//! Request-scoped Codex gateway policies shared across providers and attempts.

use super::*;
use crate::gateway::util::now_unix_millis;
use crate::shared::mutex_ext::MutexExt;
use chrono::DateTime;
use rand::Rng;
use std::time::{Duration, Instant};

pub(super) const RESPONSE_INSPECTION_LIMIT_BYTES: usize = 1024 * 1024;
pub(super) const CAPACITY_ERROR_CODE: &str = "GW_CODEX_UPSTREAM_CAPACITY";
pub(super) const RATE_LIMIT_ERROR_CODE: &str = "GW_CODEX_UPSTREAM_RATE_LIMITED";
pub(super) const FIRST_PROGRESS_TIMEOUT_ERROR_CODE: &str =
    "GW_CODEX_UPSTREAM_FIRST_PROGRESS_TIMEOUT";
pub(super) const TOTAL_TIMEOUT_ERROR_CODE: &str = "GW_CODEX_UPSTREAM_TOTAL_TIMEOUT";
pub(super) const INSPECTION_LIMIT_ERROR_CODE: &str = "GW_CODEX_RESPONSE_INSPECTION_LIMIT";

const MAX_RETRY_AFTER_MS: u64 = 60_000;
const MAX_JITTER_BACKOFF_MS: u64 = 30_000;
const CAPACITY_MESSAGE: &str = "selected model is at capacity. please try a different model.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PolicyTrigger {
    Capacity,
    Http429,
}

impl PolicyTrigger {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Capacity => "capacity",
            Self::Http429 => "http_429",
        }
    }

    pub(super) const fn reason_header(self) -> &'static str {
        match self {
            Self::Capacity => "upstream-capacity",
            Self::Http429 => "upstream-rate-limited",
        }
    }

    pub(super) const fn gateway_error_code(self) -> &'static str {
        match self {
            Self::Capacity => CAPACITY_ERROR_CODE,
            Self::Http429 => RATE_LIMIT_ERROR_CODE,
        }
    }

    pub(super) const fn payload_error_code(self) -> &'static str {
        match self {
            Self::Capacity => "upstream_capacity_policy_triggered",
            Self::Http429 => "upstream_rate_limit_policy_triggered",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TimeoutPhase {
    FirstProgress,
    Total,
}

impl TimeoutPhase {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::FirstProgress => "first_progress",
            Self::Total => "total",
        }
    }

    pub(super) const fn reason_header(self) -> &'static str {
        match self {
            Self::FirstProgress => "upstream-first-progress-timeout",
            Self::Total => "upstream-total-timeout",
        }
    }

    pub(super) const fn gateway_error_code(self) -> &'static str {
        match self {
            Self::FirstProgress => FIRST_PROGRESS_TIMEOUT_ERROR_CODE,
            Self::Total => TOTAL_TIMEOUT_ERROR_CODE,
        }
    }

    pub(super) const fn payload_error_code(self) -> &'static str {
        match self {
            Self::FirstProgress => "upstream_first_progress_timeout",
            Self::Total => "upstream_total_timeout",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct AttemptPolicyTiming {
    pub(super) sequence: u32,
    pub(super) dispatched_at: Instant,
    pub(super) dispatched_at_unix_ms: i64,
    pub(super) first_progress_deadline: Option<Instant>,
    pub(super) total_deadline: Option<Instant>,
}

impl AttemptPolicyTiming {
    pub(super) fn expired_phase(self, first_progress_seen: bool) -> Option<TimeoutPhase> {
        let now = Instant::now();
        if self.total_deadline.is_some_and(|deadline| now >= deadline) {
            return Some(TimeoutPhase::Total);
        }
        if !first_progress_seen
            && self
                .first_progress_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            return Some(TimeoutPhase::FirstProgress);
        }
        None
    }

    pub(super) fn next_deadline(
        self,
        first_progress_seen: bool,
    ) -> Option<(Instant, TimeoutPhase)> {
        let mut result = self
            .total_deadline
            .map(|deadline| (deadline, TimeoutPhase::Total));
        if !first_progress_seen {
            if let Some(first) = self.first_progress_deadline {
                if result.is_none_or(|(current, _)| first < current) {
                    result = Some((first, TimeoutPhase::FirstProgress));
                }
            }
        }
        result
    }

    pub(super) fn timeout_limit_ms(
        self,
        phase: TimeoutPhase,
        first_progress_timeout_ms: u32,
        total_timeout_ms: u32,
    ) -> u32 {
        match phase {
            TimeoutPhase::FirstProgress => first_progress_timeout_ms,
            TimeoutPhase::Total => total_timeout_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RetryReservation {
    pub(super) used: u32,
    pub(super) remaining: u32,
    pub(super) phase: &'static str,
    pub(super) budget_delay_ms: u32,
}

#[derive(Debug)]
pub(super) struct LayeredPolicyState {
    applies: bool,
    latency_guard_enabled: bool,
    first_progress_timeout_ms: u32,
    total_timeout_ms: u32,
    immediate_budget: u32,
    delayed_budget: u32,
    delayed_retry_ms: u32,
    retries_used: u32,
    attempt_sequence: u32,
    total_deadline: Option<Instant>,
}

impl LayeredPolicyState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        cli_key: &str,
        path: &str,
        latency_guard_enabled: bool,
        first_progress_timeout_ms: u32,
        total_timeout_ms: u32,
        immediate_budget: u32,
        delayed_budget: u32,
        delayed_retry_ms: u32,
    ) -> Self {
        Self {
            applies: is_managed_codex_responses_path(cli_key, path),
            latency_guard_enabled,
            first_progress_timeout_ms,
            total_timeout_ms,
            immediate_budget,
            delayed_budget,
            delayed_retry_ms,
            retries_used: 0,
            attempt_sequence: 0,
            total_deadline: None,
        }
    }

    pub(super) fn applies(&self) -> bool {
        self.applies
    }

    pub(super) fn begin_attempt(&mut self, now: Instant) -> AttemptPolicyTiming {
        self.attempt_sequence = self.attempt_sequence.saturating_add(1);
        if self.applies && self.latency_guard_enabled && self.total_timeout_ms > 0 {
            self.total_deadline.get_or_insert_with(|| {
                now.checked_add(Duration::from_millis(self.total_timeout_ms as u64))
                    .unwrap_or(now)
            });
        }
        let first_progress_deadline =
            (self.applies && self.latency_guard_enabled && self.first_progress_timeout_ms > 0)
                .then(|| {
                    now.checked_add(Duration::from_millis(self.first_progress_timeout_ms as u64))
                        .unwrap_or(now)
                });
        AttemptPolicyTiming {
            sequence: self.attempt_sequence,
            dispatched_at: now,
            dispatched_at_unix_ms: now_unix_millis().min(i64::MAX as u64) as i64,
            first_progress_deadline,
            total_deadline: self.total_deadline,
        }
    }

    pub(super) fn reserve_retry(&mut self) -> Option<RetryReservation> {
        let reservation = self.preview_retry()?;
        self.retries_used = reservation.used;
        Some(reservation)
    }

    pub(super) fn preview_retry(&self) -> Option<RetryReservation> {
        if !self.applies
            || self
                .total_deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return None;
        }
        let total = self.immediate_budget.saturating_add(self.delayed_budget);
        if self.retries_used >= total {
            return None;
        }
        let used = self.retries_used.saturating_add(1);
        let delayed = used > self.immediate_budget;
        Some(RetryReservation {
            used,
            remaining: total.saturating_sub(used),
            phase: if delayed { "delayed" } else { "immediate" },
            budget_delay_ms: if delayed { self.delayed_retry_ms } else { 0 },
        })
    }

    pub(super) fn budget_snapshot(&self) -> (u32, u32) {
        let total = self.immediate_budget.saturating_add(self.delayed_budget);
        (self.retries_used, total.saturating_sub(self.retries_used))
    }

    pub(super) fn total_expired(&self) -> bool {
        self.total_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub(super) fn remaining_total(&self) -> Option<Duration> {
        self.total_deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

pub(super) fn is_managed_codex_responses_path(cli_key: &str, path: &str) -> bool {
    cli_key == "codex" && matches!(path.trim_end_matches('/'), "/responses" | "/v1/responses")
}

pub(super) fn classify_upstream_policy(
    status: reqwest::StatusCode,
    body: &[u8],
) -> Option<PolicyTrigger> {
    if !status.is_success() {
        let text = String::from_utf8_lossy(body).to_ascii_lowercase();
        if text.contains(CAPACITY_MESSAGE)
            || (text.contains("selected model is at capacity")
                && text.contains("try a different model"))
        {
            return Some(PolicyTrigger::Capacity);
        }
    }
    (status == reqwest::StatusCode::TOO_MANY_REQUESTS).then_some(PolicyTrigger::Http429)
}

pub(super) fn action_requests_retry(action: crate::settings::CodexGatewayPolicyAction) -> bool {
    matches!(
        action,
        crate::settings::CodexGatewayPolicyAction::RetryThenPassThrough
            | crate::settings::CodexGatewayPolicyAction::RetryThen502
    )
}

pub(super) fn action_exhausts_to_502(action: crate::settings::CodexGatewayPolicyAction) -> bool {
    matches!(
        action,
        crate::settings::CodexGatewayPolicyAction::Return502
            | crate::settings::CodexGatewayPolicyAction::RetryThen502
    )
}

pub(super) fn policy_action_label(
    action: crate::settings::CodexGatewayPolicyAction,
) -> &'static str {
    match action {
        crate::settings::CodexGatewayPolicyAction::PassThrough => "pass_through",
        crate::settings::CodexGatewayPolicyAction::Return502 => "return_502",
        crate::settings::CodexGatewayPolicyAction::RetryThenPassThrough => {
            "retry_then_pass_through"
        }
        crate::settings::CodexGatewayPolicyAction::RetryThen502 => "retry_then_502",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetryDelay {
    pub(super) retry_after_raw: Option<String>,
    pub(super) retry_after_ms: Option<u64>,
    pub(super) delay_ms: u64,
    pub(super) allowed: bool,
}

pub(super) fn resolve_retry_delay(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    retry_attempt_index: u32,
) -> RetryDelay {
    if status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return RetryDelay {
            retry_after_raw: None,
            retry_after_ms: None,
            delay_ms: 0,
            allowed: true,
        };
    }

    let retry_after_raw = headers
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(raw) = retry_after_raw.as_deref() {
        if let Some(retry_after_ms) = parse_retry_after_ms(raw) {
            return RetryDelay {
                retry_after_raw,
                retry_after_ms: Some(retry_after_ms),
                delay_ms: retry_after_ms,
                allowed: retry_after_ms <= MAX_RETRY_AFTER_MS,
            };
        }
    }

    let exponent = retry_attempt_index.min(20);
    let cap = 1000u64
        .saturating_mul(1u64 << exponent)
        .min(MAX_JITTER_BACKOFF_MS);
    let delay_ms = rand::thread_rng().gen_range(0..=cap);
    RetryDelay {
        retry_after_raw,
        retry_after_ms: None,
        delay_ms,
        allowed: true,
    }
}

fn parse_retry_after_ms(raw: &str) -> Option<u64> {
    if let Ok(seconds) = raw.parse::<f64>() {
        if seconds.is_finite() && seconds >= 0.0 {
            return Some((seconds * 1000.0).round().min(u64::MAX as f64) as u64);
        }
    }
    let parsed = DateTime::parse_from_rfc2822(raw).ok()?;
    let now_ms = now_unix_millis().min(i64::MAX as u64) as i64;
    let target_ms = parsed.timestamp_millis();
    Some(target_ms.saturating_sub(now_ms).max(0) as u64)
}

pub(super) fn reserve_shared_retry(
    state: &Arc<Mutex<LayeredPolicyState>>,
) -> Option<RetryReservation> {
    state.lock_or_recover().reserve_retry()
}

pub(super) fn preview_shared_retry(
    state: &Arc<Mutex<LayeredPolicyState>>,
) -> Option<RetryReservation> {
    state.lock_or_recover().preview_retry()
}

pub(super) fn budget_snapshot(state: &Arc<Mutex<LayeredPolicyState>>) -> (u32, u32) {
    state.lock_or_recover().budget_snapshot()
}

pub(super) async fn sleep_before_retry(state: &Arc<Mutex<LayeredPolicyState>>, delay: Duration) {
    if delay.is_zero() {
        return;
    }
    let remaining = state.lock_or_recover().remaining_total();
    match remaining {
        Some(remaining) if remaining < delay => tokio::time::sleep(remaining).await,
        _ => tokio::time::sleep(delay).await,
    }
}

fn action_for_trigger<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    trigger: PolicyTrigger,
) -> crate::settings::CodexGatewayPolicyAction {
    match trigger {
        PolicyTrigger::Capacity => ctx.codex_gateway_capacity_error_action,
        PolicyTrigger::Http429 => ctx.codex_gateway_http_429_action,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_upstream_policy<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    loop_state: LoopState<'_, R>,
    status: reqwest::StatusCode,
    response_headers: HeaderMap,
    body: Bytes,
    allow_next_retry_beyond_max_attempts: &mut bool,
) -> Option<LoopControl> {
    if !ctx.layered_policy_state.lock_or_recover().applies() {
        return None;
    }
    if let Some(phase) = attempt_ctx
        .policy_timing
        .and_then(|timing| timing.expired_phase(false))
    {
        let control =
            handle_timeout_before_forward(ctx, provider_ctx, attempt_ctx, loop_state, phase).await;
        if matches!(control, LoopControl::ContinueRetry) {
            *allow_next_retry_beyond_max_attempts = true;
        }
        return Some(control);
    }
    let trigger = classify_upstream_policy(status, body.as_ref())?;
    let action = action_for_trigger(ctx, trigger);
    let retry_delay = resolve_retry_delay(
        status,
        &response_headers,
        budget_snapshot(ctx.layered_policy_state).0,
    );
    let preview = action_requests_retry(action)
        .then(|| preview_shared_retry(ctx.layered_policy_state))
        .flatten();
    let combined_delay_ms = preview
        .map(|reservation| retry_delay.delay_ms.max(reservation.budget_delay_ms as u64))
        .unwrap_or(retry_delay.delay_ms);
    let fits_total_deadline = ctx
        .layered_policy_state
        .lock_or_recover()
        .remaining_total()
        .is_none_or(|remaining| remaining >= Duration::from_millis(combined_delay_ms));
    let reservation = if retry_delay.allowed && fits_total_deadline && preview.is_some() {
        reserve_shared_retry(ctx.layered_policy_state)
    } else {
        None
    };

    if let Some(timing) = attempt_ctx.policy_timing {
        let (used, remaining) = budget_snapshot(ctx.layered_policy_state);
        update_attempt_telemetry(
            ctx.special_settings,
            timing.sequence,
            serde_json::json!({
                "policyTrigger": trigger.as_str(),
                "policyAction": policy_action_label(action),
                "retryTrigger": reservation.map(|_| trigger.as_str()),
                "retryDelayMs": reservation.map(|_| combined_delay_ms),
                "retryAfterRaw": retry_delay.retry_after_raw,
                "retryAfterMs": retry_delay.retry_after_ms,
                "retryBudgetUsed": used,
                "retryBudgetRemaining": remaining,
                "finalAction": if reservation.is_some() {
                    format!("{}_internal_retry", trigger.as_str())
                } else if action_exhausts_to_502(action) {
                    format!("{}_returned_502", trigger.as_str())
                } else {
                    format!("{}_passed_through", trigger.as_str())
                },
            }),
        );
    }

    if let Some(reservation) = reservation {
        *allow_next_retry_beyond_max_attempts = true;
        record_policy_attempt(
            ctx,
            provider_ctx,
            attempt_ctx,
            loop_state,
            trigger,
            action,
            status.as_u16(),
            "retry_same_provider",
        )
        .await;
        sleep_before_retry(
            ctx.layered_policy_state,
            Duration::from_millis(combined_delay_ms),
        )
        .await;
        tracing::info!(
            trace_id = %ctx.trace_id,
            trigger = trigger.as_str(),
            retry_budget_used = reservation.used,
            retry_budget_remaining = reservation.remaining,
            retry_delay_ms = combined_delay_ms,
            "Codex layered policy scheduled an internal retry"
        );
        return Some(LoopControl::ContinueRetry);
    }

    Some(
        finish_policy_response(
            ctx,
            provider_ctx,
            attempt_ctx,
            loop_state,
            trigger,
            action,
            status,
            response_headers,
            body,
        )
        .await,
    )
}

async fn record_policy_attempt<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    loop_state: LoopState<'_, R>,
    trigger: PolicyTrigger,
    action: crate::settings::CodexGatewayPolicyAction,
    status: u16,
    decision: &'static str,
) {
    let outcome = format!(
        "codex_gateway_policy: trigger={} action={} decision={decision}",
        trigger.as_str(),
        policy_action_label(action)
    );
    loop_state.attempts.push(FailoverAttempt {
        provider_id: provider_ctx.provider_id,
        provider_name: provider_ctx.provider_name_base.clone(),
        base_url: provider_ctx.provider_base_url_base.clone(),
        outcome: outcome.clone(),
        status: Some(status),
        provider_index: Some(provider_ctx.provider_index),
        retry_index: Some(attempt_ctx.retry_index),
        session_reuse: provider_ctx.session_reuse,
        error_category: Some(ErrorCategory::SystemError.as_str()),
        error_code: Some(trigger.gateway_error_code()),
        decision: Some(decision),
        reason: Some(format!(
            "trigger={} action={}",
            trigger.as_str(),
            policy_action_label(action)
        )),
        selection_method: dc::selection_method(
            provider_ctx.provider_index,
            attempt_ctx.retry_index,
            provider_ctx.session_reuse,
        ),
        reason_code: Some(ErrorCategory::SystemError.reason_code()),
        attempt_started_ms: Some(attempt_ctx.attempt_started_ms),
        attempt_duration_ms: Some(attempt_ctx.attempt_started.elapsed().as_millis()),
        circuit_state_before: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_state_after: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_failure_count: Some(attempt_ctx.circuit_before.failure_count),
        circuit_failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        circuit_recover_at_unix: None,
        circuit_trigger_error_code: None,
        provider_bridged: Some(provider_ctx.provider_bridged),
        timeout_secs: None,
    });
    emit_attempt_event_and_log(
        ctx,
        provider_ctx,
        attempt_ctx,
        outcome,
        Some(status),
        AttemptCircuitFields {
            state_before: Some(attempt_ctx.circuit_before.state.as_str()),
            state_after: Some(attempt_ctx.circuit_before.state.as_str()),
            failure_count: Some(attempt_ctx.circuit_before.failure_count),
            failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        },
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn finish_policy_response<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    mut loop_state: LoopState<'_, R>,
    trigger: PolicyTrigger,
    action: crate::settings::CodexGatewayPolicyAction,
    upstream_status: reqwest::StatusCode,
    mut response_headers: HeaderMap,
    upstream_body: Bytes,
) -> LoopControl {
    let return_502 = action_exhausts_to_502(action);
    let client_status = if return_502 {
        StatusCode::BAD_GATEWAY
    } else {
        StatusCode::from_u16(upstream_status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY)
    };
    let decision = if return_502 { "abort" } else { "pass_through" };
    record_policy_attempt(
        ctx,
        provider_ctx,
        attempt_ctx,
        loop_state.reborrow(),
        trigger,
        action,
        client_status.as_u16(),
        decision,
    )
    .await;

    *loop_state.last_outcome = Some(AttemptOutcome::new(
        ErrorCategory::SystemError.as_str(),
        trigger.gateway_error_code(),
    ));
    let duration_ms = ctx.started.elapsed().as_millis();
    emit_request_event_and_enqueue_request_log(
        RequestEndArgs::from_context(RequestEndContextArgs {
            deps: RequestEndDeps::new(
                &ctx.state.app,
                &ctx.state.db,
                &ctx.state.log_tx,
                &ctx.state.plugin_pipeline,
                &ctx.state.active_requests,
            ),
            trace_id: ctx.trace_id.as_str(),
            cli_key: ctx.cli_key.as_str(),
            method: ctx.method_hint.as_str(),
            path: ctx.forwarded_path.as_str(),
            observe: ctx.observe,
            query: ctx.query.as_deref(),
            excluded_from_stats: false,
            duration_ms,
            attempts: loop_state.attempts.as_slice(),
            special_settings_json: response_fixer::special_settings_json(ctx.special_settings),
            session_id: ctx.session_id.clone(),
            requested_model: ctx.requested_model.clone(),
            created_at_ms: ctx.created_at_ms,
            created_at: ctx.created_at,
        })
        .with_completion(RequestCompletion::failure_with_ttfb(
            client_status.as_u16(),
            Some(ErrorCategory::SystemError.as_str()),
            trigger.gateway_error_code(),
            duration_ms,
        )),
    )
    .await;
    loop_state.abort_guard.disarm();

    if !return_502 {
        strip_hop_headers(&mut response_headers);
        return LoopControl::Return(build_response(
            client_status,
            &response_headers,
            ctx.trace_id.as_str(),
            Body::from(upstream_body),
        ));
    }

    let (used, remaining) = budget_snapshot(ctx.layered_policy_state);
    let payload = serde_json::json!({
        "trace_id": ctx.trace_id,
        "error": {
            "type": "codex_retry_gateway_upstream_policy_error",
            "code": trigger.payload_error_code(),
            "message": match trigger {
                PolicyTrigger::Capacity => "上游模型容量不足，AIO 已按 Capacity 策略终止本次请求。",
                PolicyTrigger::Http429 => "上游返回 HTTP 429，AIO 已按限流策略终止本次请求。",
            },
        },
        "retry_attempt_index": used,
        "retry_attempts_used": used,
        "retry_attempts_remaining": remaining,
    });
    let mut response = (client_status, axum::Json(payload)).into_response();
    response.headers_mut().insert(
        "x-codex-retry-gateway-reason",
        HeaderValue::from_static(trigger.reason_header()),
    );
    if let Ok(value) = HeaderValue::from_str(ctx.trace_id.as_str()) {
        response.headers_mut().insert("x-trace-id", value);
    }
    LoopControl::Return(response)
}

pub(super) async fn handle_timeout_before_forward<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    loop_state: LoopState<'_, R>,
    phase: TimeoutPhase,
) -> LoopControl {
    let should_retry = phase == TimeoutPhase::FirstProgress
        && ctx.codex_gateway_first_progress_action
            == crate::settings::CodexGatewayFirstProgressAction::RetryThen502;
    let preview = should_retry
        .then(|| preview_shared_retry(ctx.layered_policy_state))
        .flatten();
    let fits_total = preview.is_some_and(|reservation| {
        ctx.layered_policy_state
            .lock_or_recover()
            .remaining_total()
            .is_none_or(|remaining| {
                remaining >= Duration::from_millis(reservation.budget_delay_ms as u64)
            })
    });
    let reservation = if fits_total {
        reserve_shared_retry(ctx.layered_policy_state)
    } else {
        None
    };

    if let Some(timing) = attempt_ctx.policy_timing {
        let limit_ms = timing.timeout_limit_ms(
            phase,
            ctx.codex_gateway_first_progress_timeout_ms,
            ctx.codex_gateway_total_timeout_ms,
        );
        let (used, remaining) = budget_snapshot(ctx.layered_policy_state);
        update_attempt_telemetry(
            ctx.special_settings,
            timing.sequence,
            serde_json::json!({
                "policyTrigger": format!("{}_timeout", phase.as_str()),
                "policyAction": if reservation.is_some() { "retry_then_502" } else { "return_502" },
                "retryTrigger": reservation.map(|_| "first_progress_timeout"),
                "retryDelayMs": reservation.map(|value| value.budget_delay_ms),
                "retryBudgetUsed": used,
                "retryBudgetRemaining": remaining,
                "timeoutPhase": phase.as_str(),
                "timeoutLimitMs": limit_ms,
                "finalAction": if reservation.is_some() {
                    "first_progress_timeout_internal_retry"
                } else if phase == TimeoutPhase::FirstProgress {
                    "first_progress_timeout_returned_502"
                } else {
                    "total_timeout_returned_502"
                },
            }),
        );
    }

    if let Some(reservation) = reservation {
        record_timeout_attempt(
            ctx,
            provider_ctx,
            attempt_ctx,
            loop_state,
            phase,
            "retry_same_provider",
        )
        .await;
        sleep_before_retry(
            ctx.layered_policy_state,
            Duration::from_millis(reservation.budget_delay_ms as u64),
        )
        .await;
        return LoopControl::ContinueRetry;
    }

    finish_timeout_response(ctx, provider_ctx, Some(attempt_ctx), loop_state, phase).await
}

async fn record_timeout_attempt<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    loop_state: LoopState<'_, R>,
    phase: TimeoutPhase,
    decision: &'static str,
) {
    let outcome = format!(
        "codex_gateway_timeout: phase={} decision={decision}",
        phase.as_str()
    );
    loop_state.attempts.push(FailoverAttempt {
        provider_id: provider_ctx.provider_id,
        provider_name: provider_ctx.provider_name_base.clone(),
        base_url: provider_ctx.provider_base_url_base.clone(),
        outcome: outcome.clone(),
        status: Some(StatusCode::BAD_GATEWAY.as_u16()),
        provider_index: Some(provider_ctx.provider_index),
        retry_index: Some(attempt_ctx.retry_index),
        session_reuse: provider_ctx.session_reuse,
        error_category: Some(ErrorCategory::SystemError.as_str()),
        error_code: Some(phase.gateway_error_code()),
        decision: Some(decision),
        reason: Some(format!("timeout_phase={}", phase.as_str())),
        selection_method: dc::selection_method(
            provider_ctx.provider_index,
            attempt_ctx.retry_index,
            provider_ctx.session_reuse,
        ),
        reason_code: Some(ErrorCategory::SystemError.reason_code()),
        attempt_started_ms: Some(attempt_ctx.attempt_started_ms),
        attempt_duration_ms: Some(attempt_ctx.attempt_started.elapsed().as_millis()),
        circuit_state_before: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_state_after: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_failure_count: Some(attempt_ctx.circuit_before.failure_count),
        circuit_failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        circuit_recover_at_unix: None,
        circuit_trigger_error_code: None,
        provider_bridged: Some(provider_ctx.provider_bridged),
        timeout_secs: None,
    });
    emit_attempt_event_and_log(
        ctx,
        provider_ctx,
        attempt_ctx,
        outcome,
        Some(StatusCode::BAD_GATEWAY.as_u16()),
        AttemptCircuitFields {
            state_before: Some(attempt_ctx.circuit_before.state.as_str()),
            state_after: Some(attempt_ctx.circuit_before.state.as_str()),
            failure_count: Some(attempt_ctx.circuit_before.failure_count),
            failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        },
    )
    .await;
}

async fn finish_timeout_response<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: Option<AttemptCtx<'_>>,
    mut loop_state: LoopState<'_, R>,
    phase: TimeoutPhase,
) -> LoopControl {
    if let Some(attempt_ctx) = attempt_ctx {
        record_timeout_attempt(
            ctx,
            provider_ctx,
            attempt_ctx,
            loop_state.reborrow(),
            phase,
            "abort",
        )
        .await;
    }
    *loop_state.last_outcome = Some(AttemptOutcome::new(
        ErrorCategory::SystemError.as_str(),
        phase.gateway_error_code(),
    ));
    let duration_ms = ctx.started.elapsed().as_millis();
    emit_request_event_and_enqueue_request_log(
        RequestEndArgs::from_context(RequestEndContextArgs {
            deps: RequestEndDeps::new(
                &ctx.state.app,
                &ctx.state.db,
                &ctx.state.log_tx,
                &ctx.state.plugin_pipeline,
                &ctx.state.active_requests,
            ),
            trace_id: ctx.trace_id.as_str(),
            cli_key: ctx.cli_key.as_str(),
            method: ctx.method_hint.as_str(),
            path: ctx.forwarded_path.as_str(),
            observe: ctx.observe,
            query: ctx.query.as_deref(),
            excluded_from_stats: false,
            duration_ms,
            attempts: loop_state.attempts.as_slice(),
            special_settings_json: response_fixer::special_settings_json(ctx.special_settings),
            session_id: ctx.session_id.clone(),
            requested_model: ctx.requested_model.clone(),
            created_at_ms: ctx.created_at_ms,
            created_at: ctx.created_at,
        })
        .with_completion(RequestCompletion::failure_with_ttfb(
            StatusCode::BAD_GATEWAY.as_u16(),
            Some(ErrorCategory::SystemError.as_str()),
            phase.gateway_error_code(),
            duration_ms,
        )),
    )
    .await;
    loop_state.abort_guard.disarm();

    let (used, remaining) = budget_snapshot(ctx.layered_policy_state);
    let timeout_limit_ms = match phase {
        TimeoutPhase::FirstProgress => ctx.codex_gateway_first_progress_timeout_ms,
        TimeoutPhase::Total => ctx.codex_gateway_total_timeout_ms,
    };
    let payload = serde_json::json!({
        "trace_id": ctx.trace_id,
        "error": {
            "type": "codex_retry_gateway_upstream_policy_error",
            "code": phase.payload_error_code(),
            "message": match phase {
                TimeoutPhase::FirstProgress => "上游未在限制时间内产生首个有效输出。",
                TimeoutPhase::Total => "上游请求超过总耗时硬截止线。",
            },
        },
        "retry_attempt_index": used,
        "retry_attempts_used": used,
        "retry_attempts_remaining": remaining,
        "timeout_limit_ms": timeout_limit_ms,
        "timeout_phase": phase.as_str(),
    });
    let mut response = (StatusCode::BAD_GATEWAY, axum::Json(payload)).into_response();
    response.headers_mut().insert(
        "x-codex-retry-gateway-reason",
        HeaderValue::from_static(phase.reason_header()),
    );
    if let Ok(value) = HeaderValue::from_str(ctx.trace_id.as_str()) {
        response.headers_mut().insert("x-trace-id", value);
    }
    LoopControl::Return(response)
}

pub(super) async fn maybe_finish_expired_total<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    prepared: &super::provider_iterator::PreparedProvider,
    loop_state: LoopState<'_, R>,
) -> Option<Response> {
    if !ctx.layered_policy_state.lock_or_recover().total_expired() {
        return None;
    }
    response_fixer::push_special_setting(
        ctx.special_settings,
        serde_json::json!({
            "type": "codex_gateway_policy_timeout",
            "scope": "request",
            "timeoutPhase": "total",
            "timeoutLimitMs": ctx.codex_gateway_total_timeout_ms,
            "responseForwardingStarted": false,
            "finalAction": "total_timeout_returned_502_before_dispatch",
        }),
    );
    let provider_ctx = ProviderCtx {
        provider_id: prepared.provider_id,
        provider_name_base: &prepared.provider_name_base,
        provider_base_url_base: &prepared.provider_base_url_base,
        auth_mode: prepared.auth_mode.as_str(),
        provider_index: prepared.provider_index,
        provider_bridged: prepared.provider_bridged,
        session_reuse: prepared.session_reuse,
        stream_idle_timeout_seconds: prepared.stream_idle_timeout_seconds,
        claude_model_mapping: prepared.claude_model_mapping.as_ref(),
    };
    match finish_timeout_response(ctx, provider_ctx, None, loop_state, TimeoutPhase::Total).await {
        LoopControl::Return(response) => Some(response),
        _ => unreachable!("terminal total timeout must return a response"),
    }
}

pub(super) async fn handle_inspection_limit_before_forward<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    loop_state: LoopState<'_, R>,
) -> LoopControl {
    if let Some(timing) = attempt_ctx.policy_timing {
        let (used, remaining) = budget_snapshot(ctx.layered_policy_state);
        update_attempt_telemetry(
            ctx.special_settings,
            timing.sequence,
            serde_json::json!({
                "policyTrigger": "response_inspection_limit",
                "policyAction": "return_502",
                "retryBudgetUsed": used,
                "retryBudgetRemaining": remaining,
                "inspectionLimitBytes": RESPONSE_INSPECTION_LIMIT_BYTES,
                "finalAction": "response_inspection_limit_exceeded",
            }),
        );
    }

    let outcome = "codex_gateway_response_inspection_limit: decision=abort".to_string();
    loop_state.attempts.push(FailoverAttempt {
        provider_id: provider_ctx.provider_id,
        provider_name: provider_ctx.provider_name_base.clone(),
        base_url: provider_ctx.provider_base_url_base.clone(),
        outcome: outcome.clone(),
        status: Some(StatusCode::BAD_GATEWAY.as_u16()),
        provider_index: Some(provider_ctx.provider_index),
        retry_index: Some(attempt_ctx.retry_index),
        session_reuse: provider_ctx.session_reuse,
        error_category: Some(ErrorCategory::SystemError.as_str()),
        error_code: Some(INSPECTION_LIMIT_ERROR_CODE),
        decision: Some("abort"),
        reason: Some(format!(
            "inspection_limit_bytes={RESPONSE_INSPECTION_LIMIT_BYTES}"
        )),
        selection_method: dc::selection_method(
            provider_ctx.provider_index,
            attempt_ctx.retry_index,
            provider_ctx.session_reuse,
        ),
        reason_code: Some(ErrorCategory::SystemError.reason_code()),
        attempt_started_ms: Some(attempt_ctx.attempt_started_ms),
        attempt_duration_ms: Some(attempt_ctx.attempt_started.elapsed().as_millis()),
        circuit_state_before: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_state_after: Some(attempt_ctx.circuit_before.state.as_str()),
        circuit_failure_count: Some(attempt_ctx.circuit_before.failure_count),
        circuit_failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        circuit_recover_at_unix: None,
        circuit_trigger_error_code: None,
        provider_bridged: Some(provider_ctx.provider_bridged),
        timeout_secs: None,
    });
    emit_attempt_event_and_log(
        ctx,
        provider_ctx,
        attempt_ctx,
        outcome,
        Some(StatusCode::BAD_GATEWAY.as_u16()),
        AttemptCircuitFields {
            state_before: Some(attempt_ctx.circuit_before.state.as_str()),
            state_after: Some(attempt_ctx.circuit_before.state.as_str()),
            failure_count: Some(attempt_ctx.circuit_before.failure_count),
            failure_threshold: Some(attempt_ctx.circuit_before.failure_threshold),
        },
    )
    .await;
    *loop_state.last_outcome = Some(AttemptOutcome::new(
        ErrorCategory::SystemError.as_str(),
        INSPECTION_LIMIT_ERROR_CODE,
    ));

    let duration_ms = ctx.started.elapsed().as_millis();
    emit_request_event_and_enqueue_request_log(
        RequestEndArgs::from_context(RequestEndContextArgs {
            deps: RequestEndDeps::new(
                &ctx.state.app,
                &ctx.state.db,
                &ctx.state.log_tx,
                &ctx.state.plugin_pipeline,
                &ctx.state.active_requests,
            ),
            trace_id: ctx.trace_id.as_str(),
            cli_key: ctx.cli_key.as_str(),
            method: ctx.method_hint.as_str(),
            path: ctx.forwarded_path.as_str(),
            observe: ctx.observe,
            query: ctx.query.as_deref(),
            excluded_from_stats: false,
            duration_ms,
            attempts: loop_state.attempts.as_slice(),
            special_settings_json: response_fixer::special_settings_json(ctx.special_settings),
            session_id: ctx.session_id.clone(),
            requested_model: ctx.requested_model.clone(),
            created_at_ms: ctx.created_at_ms,
            created_at: ctx.created_at,
        })
        .with_completion(RequestCompletion::failure_with_ttfb(
            StatusCode::BAD_GATEWAY.as_u16(),
            Some(ErrorCategory::SystemError.as_str()),
            INSPECTION_LIMIT_ERROR_CODE,
            duration_ms,
        )),
    )
    .await;
    loop_state.abort_guard.disarm();

    let payload = serde_json::json!({
        "trace_id": ctx.trace_id,
        "error": {
            "message": format!(
                "AIO could not safely inspect an SSE event larger than {} bytes",
                RESPONSE_INSPECTION_LIMIT_BYTES
            ),
            "type": "codex_retry_gateway_error",
            "code": "response_inspection_limit_exceeded",
            "inspection_limit_bytes": RESPONSE_INSPECTION_LIMIT_BYTES,
        }
    });
    let mut response = (StatusCode::BAD_GATEWAY, axum::Json(payload)).into_response();
    response.headers_mut().insert(
        "x-codex-retry-gateway-reason",
        HeaderValue::from_static("response-inspection-limit-exceeded"),
    );
    if let Ok(value) = HeaderValue::from_str(ctx.trace_id.as_str()) {
        response.headers_mut().insert("x-trace-id", value);
    }
    LoopControl::Return(response)
}

pub(super) fn record_attempt_dispatch(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    timing: AttemptPolicyTiming,
    provider_id: i64,
    provider_name: &str,
    retry_index: u32,
) {
    response_fixer::push_special_setting(
        special_settings,
        serde_json::json!({
            "type": "codex_gateway_policy_attempt",
            "scope": "attempt",
            "attemptSequence": timing.sequence,
            "providerId": provider_id,
            "providerName": provider_name,
            "retryAttemptNumber": retry_index,
            "upstreamFetchStartedAtMs": timing.dispatched_at_unix_ms,
            "upstreamHttpStatus": null,
            "firstProgressAtMs": null,
            "timeToFirstProgressMs": null,
            "policyTrigger": null,
            "policyAction": null,
            "retryTrigger": null,
            "retryDelayMs": null,
            "retryAfterRaw": null,
            "retryAfterMs": null,
            "retryBudgetUsed": null,
            "retryBudgetRemaining": null,
            "timeoutPhase": null,
            "timeoutLimitMs": null,
            "timeoutResponseControlLost": false,
            "responseForwardingStarted": false,
            "finalAction": "upstream_dispatched",
        }),
    );
}

pub(super) fn update_attempt_telemetry(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    sequence: u32,
    updates: serde_json::Value,
) {
    let Some(updates) = updates.as_object() else {
        return;
    };
    let mut settings = special_settings.lock_or_recover();
    let Some(entry) = settings.iter_mut().rev().find(|entry| {
        entry.get("type").and_then(serde_json::Value::as_str)
            == Some("codex_gateway_policy_attempt")
            && entry
                .get("attemptSequence")
                .and_then(serde_json::Value::as_u64)
                == Some(sequence as u64)
    }) else {
        return;
    };
    let Some(entry) = entry.as_object_mut() else {
        return;
    };
    for (key, value) in updates {
        entry.insert(key.clone(), value.clone());
    }
}

pub(super) fn mark_attempt_final_action_if_unset(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    sequence: u32,
    final_action: &'static str,
) {
    let mut settings = special_settings.lock_or_recover();
    let Some(entry) = settings.iter_mut().rev().find(|entry| {
        entry.get("type").and_then(serde_json::Value::as_str)
            == Some("codex_gateway_policy_attempt")
            && entry
                .get("attemptSequence")
                .and_then(serde_json::Value::as_u64)
                == Some(sequence as u64)
    }) else {
        return;
    };
    if entry.get("finalAction").and_then(serde_json::Value::as_str) == Some("upstream_dispatched") {
        entry["finalAction"] = serde_json::Value::String(final_action.to_string());
    }
}

pub(super) fn mark_upstream_response(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    timing: AttemptPolicyTiming,
    status: u16,
) {
    update_attempt_telemetry(
        special_settings,
        timing.sequence,
        serde_json::json!({
            "upstreamHttpStatus": status,
        }),
    );
}

pub(super) fn mark_first_progress(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    timing: AttemptPolicyTiming,
) {
    let at_ms = now_unix_millis().min(i64::MAX as u64) as i64;
    let elapsed_ms = timing
        .dispatched_at
        .elapsed()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    update_attempt_telemetry(
        special_settings,
        timing.sequence,
        serde_json::json!({
            "firstProgressAtMs": at_ms,
            "timeToFirstProgressMs": elapsed_ms,
        }),
    );
}

pub(super) fn mark_response_forwarding(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    sequence: u32,
) {
    let at_ms = now_unix_millis().min(i64::MAX as u64) as i64;
    update_attempt_telemetry(
        special_settings,
        sequence,
        serde_json::json!({
            "clientHeadersSentAtMs": at_ms,
            "responseForwardingStarted": true,
            "finalAction": "forwarded",
        }),
    );
}

pub(super) fn mark_client_first_write(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    timing: AttemptPolicyTiming,
) {
    let at_ms = now_unix_millis().min(i64::MAX as u64) as i64;
    let elapsed_ms = timing
        .dispatched_at
        .elapsed()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    update_attempt_telemetry(
        special_settings,
        timing.sequence,
        serde_json::json!({
            "clientFirstWriteAtMs": at_ms,
            "timeToClientFirstWriteMs": elapsed_ms,
            "responseForwardingStarted": true,
        }),
    );
}

pub(super) fn meaningful_progress_from_json(value: &serde_json::Value) -> bool {
    let observation = super::codex_reasoning_guard::observe_response(value);
    stream_or_chat_payload_has_progress(value)
        || observation.structure.has_output_text
        || observation.structure.has_final_answer
        || observation.structure.has_tool_call
        || (observation.structure.has_commentary && contains_nonempty_progress_text(value))
}

fn stream_or_chat_payload_has_progress(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let event_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if event_type.contains("tool_call") || event_type.contains("function_call") {
        return true;
    }

    let event_text = ["delta", "text", "content", "output_text"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(serde_json::Value::as_str))
        .any(|text| !text.trim().is_empty());
    if (event_type.contains("output_text.delta")
        || event_type.contains("message.delta")
        || event_type.contains("content.delta")
        || event_type.contains("commentary"))
        && event_text
    {
        return true;
    }

    object
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|choices| {
            choices.iter().any(|choice| {
                ["/delta/content", "/message/content"]
                    .iter()
                    .filter_map(|pointer| {
                        choice.pointer(pointer).and_then(serde_json::Value::as_str)
                    })
                    .any(|text| !text.trim().is_empty())
                    || choice
                        .pointer("/delta/tool_calls")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|calls| !calls.is_empty())
                    || choice
                        .pointer("/delta/function_call")
                        .is_some_and(|call| !call.is_null())
            })
        })
}

fn contains_nonempty_progress_text(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            (matches!(key.as_str(), "delta" | "text" | "content" | "output_text")
                && value.as_str().is_some_and(|text| !text.trim().is_empty()))
                || contains_nonempty_progress_text(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_nonempty_progress_text),
        _ => false,
    }
}

pub(super) struct SseProgressInspector {
    buffer: Vec<u8>,
    oversized: bool,
    declared_sse: bool,
    sse_like: bool,
}

impl SseProgressInspector {
    pub(super) fn new() -> Self {
        Self::with_declared_sse(true)
    }

    pub(super) fn new_candidate() -> Self {
        Self::with_declared_sse(false)
    }

    fn with_declared_sse(declared_sse: bool) -> Self {
        Self {
            buffer: Vec::new(),
            oversized: false,
            declared_sse,
            sse_like: declared_sse,
        }
    }

    pub(super) fn ingest(&mut self, bytes: &[u8]) -> Result<bool, ()> {
        if self.oversized {
            return Err(());
        }
        let mut progress = false;
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let available = RESPONSE_INSPECTION_LIMIT_BYTES.saturating_sub(self.buffer.len());
            if available == 0 {
                self.oversized = true;
                self.buffer.clear();
                return Err(());
            }
            let take = remaining.len().min(available);
            self.buffer.extend_from_slice(&remaining[..take]);
            remaining = &remaining[take..];

            while let Some(end) = find_sse_event_end(self.buffer.as_slice(), false) {
                let event = self.buffer.drain(..end).collect::<Vec<_>>();
                let inspected =
                    inspect_sse_event(event.as_slice(), self.declared_sse || self.sse_like);
                self.sse_like |= inspected.recognized_sse;
                progress |= inspected.meaningful_progress;
            }

            if !self.declared_sse
                && !self.sse_like
                && !buffer_could_be_sse_candidate(self.buffer.as_slice())
            {
                return Ok(progress
                    || buffer_has_non_bom_content(self.buffer.as_slice())
                    || remaining.iter().any(|byte| !byte.is_ascii_whitespace()));
            }
            if !remaining.is_empty() && self.buffer.len() >= RESPONSE_INSPECTION_LIMIT_BYTES {
                self.oversized = true;
                self.buffer.clear();
                return Err(());
            }
        }
        if !self.declared_sse
            && !self.sse_like
            && !buffer_could_be_sse_candidate(self.buffer.as_slice())
        {
            progress |= buffer_has_non_bom_content(self.buffer.as_slice());
        }
        Ok(progress)
    }

    pub(super) fn finish(&mut self) -> Result<bool, ()> {
        if self.oversized {
            return Err(());
        }
        if self.buffer.is_empty() {
            return Ok(false);
        }
        if self.buffer.len() > RESPONSE_INSPECTION_LIMIT_BYTES {
            return Err(());
        }
        let mut progress = false;
        while let Some(end) = find_sse_event_end(self.buffer.as_slice(), true) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            let inspected = inspect_sse_event(event.as_slice(), self.declared_sse || self.sse_like);
            self.sse_like |= inspected.recognized_sse;
            progress |= inspected.meaningful_progress;
        }
        if !self.buffer.is_empty() && !self.sse_like {
            progress |= buffer_has_non_bom_content(self.buffer.as_slice());
        }
        self.buffer.clear();
        Ok(progress)
    }
}

pub(super) fn find_sse_event_end(buffer: &[u8], allow_trailing_cr: bool) -> Option<usize> {
    let mut index = 0;
    while index < buffer.len() {
        let first_len = sse_line_ending_len(buffer, index, allow_trailing_cr);
        if first_len == 0 {
            index += 1;
            continue;
        }
        let second_index = index + first_len;
        let second_len = sse_line_ending_len(buffer, second_index, allow_trailing_cr);
        if second_len > 0 {
            return Some(second_index + second_len);
        }
        index = second_index;
    }
    None
}

fn sse_line_ending_len(buffer: &[u8], index: usize, allow_trailing_cr: bool) -> usize {
    match buffer.get(index).copied() {
        Some(b'\n') => 1,
        Some(b'\r') if index + 1 >= buffer.len() => usize::from(allow_trailing_cr),
        Some(b'\r') if buffer[index + 1] == b'\n' => 2,
        Some(b'\r') => 1,
        _ => 0,
    }
}

struct SseEventInspection {
    meaningful_progress: bool,
    recognized_sse: bool,
}

fn inspect_sse_event(event: &[u8], assume_sse: bool) -> SseEventInspection {
    let event = event.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(event);
    let Ok(text) = std::str::from_utf8(event) else {
        return SseEventInspection {
            meaningful_progress: !assume_sse
                && event.iter().any(|byte| !byte.is_ascii_whitespace()),
            recognized_sse: false,
        };
    };
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut data = Vec::new();
    for line in normalized.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start());
        }
    }
    if !data.is_empty() {
        let payload = data.join("\n");
        if payload.trim() == "[DONE]" {
            return SseEventInspection {
                meaningful_progress: false,
                recognized_sse: true,
            };
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) {
            return SseEventInspection {
                meaningful_progress: meaningful_progress_from_json(&value),
                recognized_sse: true,
            };
        }
    }
    SseEventInspection {
        meaningful_progress: !assume_sse && !normalized.trim().is_empty(),
        recognized_sse: false,
    }
}

fn buffer_has_non_bom_content(buffer: &[u8]) -> bool {
    let buffer = buffer.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(buffer);
    buffer.iter().any(|byte| !byte.is_ascii_whitespace())
}

fn buffer_could_be_sse_candidate(buffer: &[u8]) -> bool {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    if buffer.len() < BOM.len() && BOM.starts_with(buffer) {
        return true;
    }
    let buffer = buffer.strip_prefix(BOM).unwrap_or(buffer);
    if buffer.is_empty() {
        return true;
    }
    let preview = &buffer[..buffer.len().min(4096)];
    let Ok(preview) = std::str::from_utf8(preview) else {
        return false;
    };
    preview
        .split(['\r', '\n'])
        .filter(|line| !line.is_empty())
        .all(|line| {
            ["data:", "event:", "id:", "retry:", ":"]
                .iter()
                .any(|prefix| prefix.starts_with(line) || line.starts_with(prefix))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn request_budget_is_shared_and_does_not_reset() {
        let mut state = LayeredPolicyState::new("codex", "/v1/responses", false, 0, 0, 1, 1, 250);
        assert_eq!(state.reserve_retry().unwrap().phase, "immediate");
        let delayed = state.reserve_retry().unwrap();
        assert_eq!(delayed.phase, "delayed");
        assert_eq!(delayed.budget_delay_ms, 250);
        assert!(state.reserve_retry().is_none());
    }

    #[test]
    fn expired_total_deadline_cannot_consume_retry_budget() {
        let mut state = LayeredPolicyState::new("codex", "/v1/responses", true, 0, 1, 1, 1, 0);
        state.total_deadline = Instant::now().checked_sub(Duration::from_millis(1));
        assert!(state.reserve_retry().is_none());
        assert_eq!(state.budget_snapshot(), (0, 2));
    }

    #[test]
    fn default_attempt_final_action_does_not_overwrite_policy_outcome() {
        let mut state = LayeredPolicyState::new("codex", "/v1/responses", false, 0, 0, 1, 0, 0);
        let timing = state.begin_attempt(Instant::now());
        let settings = Arc::new(Mutex::new(Vec::new()));
        record_attempt_dispatch(&settings, timing, 7, "Provider", 1);
        mark_attempt_final_action_if_unset(&settings, timing.sequence, "upstream_error_handled");
        update_attempt_telemetry(
            &settings,
            timing.sequence,
            serde_json::json!({ "finalAction": "capacity_returned_502" }),
        );
        mark_attempt_final_action_if_unset(
            &settings,
            timing.sequence,
            "upstream_success_processed",
        );

        let settings = settings.lock_or_recover();
        assert_eq!(settings[0]["finalAction"], "capacity_returned_502");
    }

    #[test]
    fn capacity_has_priority_over_generic_429() {
        assert_eq!(
            classify_upstream_policy(
                reqwest::StatusCode::TOO_MANY_REQUESTS,
                br#"{\"error\":{\"message\":\"Selected model is at capacity. Please try a different model.\"}}"#,
            ),
            Some(PolicyTrigger::Capacity)
        );
    }

    #[test]
    fn retry_after_supports_seconds_and_http_date() {
        let mut headers = HeaderMap::new();
        headers.insert(header::RETRY_AFTER, HeaderValue::from_static("1.5"));
        let delay = resolve_retry_delay(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, 0);
        assert_eq!(delay.retry_after_ms, Some(1500));

        headers.insert(
            header::RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2099 07:28:00 GMT"),
        );
        let delay = resolve_retry_delay(reqwest::StatusCode::TOO_MANY_REQUESTS, &headers, 0);
        assert!(delay
            .retry_after_ms
            .is_some_and(|value| value > MAX_RETRY_AFTER_MS));
        assert!(!delay.allowed);
    }

    #[test]
    fn mixed_sse_line_endings_and_split_bom_detect_progress() {
        let mut inspector = SseProgressInspector::new();
        assert!(!inspector.ingest(&[0xEF]).unwrap());
        assert!(!inspector.ingest(&[0xBB, 0xBF]).unwrap());
        assert!(inspector
            .ingest(b"event: response.output_text.delta\rdata: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\r")
            .unwrap());
    }

    #[test]
    fn reasoning_only_event_is_not_progress() {
        let mut inspector = SseProgressInspector::new();
        assert!(!inspector
            .ingest(b"event: response.reasoning_summary_text.delta\ndata: {\"type\":\"reasoning\",\"delta\":\"hidden\"}\n\n")
            .unwrap());
    }

    #[test]
    fn lifecycle_event_is_not_progress() {
        let mut inspector = SseProgressInspector::new();
        assert!(!inspector
            .ingest(b"data: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"r1\"}}\n\n")
            .unwrap());
    }

    #[test]
    fn commentary_and_tool_events_are_progress() {
        let mut commentary = SseProgressInspector::new();
        assert!(commentary
            .ingest(
                b"data: {\"type\":\"response.commentary_text.delta\",\"delta\":\"checking\"}\n\n"
            )
            .unwrap());

        let mut tool = SseProgressInspector::new();
        assert!(tool
            .ingest(
                b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{\"}\n\n"
            )
            .unwrap());
    }

    #[test]
    fn chat_completion_choice_content_is_progress() {
        assert!(meaningful_progress_from_json(&serde_json::json!({
            "choices": [{"delta": {"content": "hello"}}]
        })));
    }

    #[test]
    fn oversized_event_fails_closed() {
        let mut inspector = SseProgressInspector::new();
        assert!(inspector
            .ingest(&vec![b'x'; RESPONSE_INSPECTION_LIMIT_BYTES + 1])
            .is_err());
    }

    #[test]
    fn oversized_plain_text_candidate_reports_progress_without_buffering_everything() {
        let mut inspector = SseProgressInspector::new_candidate();
        let body = vec![b'x'; RESPONSE_INSPECTION_LIMIT_BYTES + 1];

        assert!(inspector.ingest(&body).unwrap());
    }

    #[test]
    fn large_chunk_with_many_small_sse_events_stays_within_per_event_limit() {
        let mut inspector = SseProgressInspector::new();
        let event = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n";
        let mut body = Vec::with_capacity(RESPONSE_INSPECTION_LIMIT_BYTES + event.len());
        while body.len() <= RESPONSE_INSPECTION_LIMIT_BYTES {
            body.extend_from_slice(event);
        }

        assert!(inspector.ingest(&body).unwrap());
        assert!(!inspector.finish().unwrap());
    }

    #[test]
    fn unconfirmed_sse_candidate_falls_back_to_plain_text_progress() {
        let mut inspector = SseProgressInspector::new_candidate();
        assert!(inspector.ingest(b"data: ordinary text\n\n").unwrap());

        let mut declared = SseProgressInspector::new();
        assert!(!declared.ingest(b"data: ordinary text\n\n").unwrap());
    }
}
