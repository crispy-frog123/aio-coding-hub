//! Usage: Handle successful event-stream upstream responses inside `failover_loop::run`.

use super::*;
use crate::gateway::proxy::{gemini_oauth, protocol_bridge, provider_router};
use crate::shared::mutex_ext::MutexExt;
use futures_core::Stream;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkWaitTimeout {
    Idle,
    Policy(layered_policy::TimeoutPhase),
}

async fn read_next_chunk(
    resp: &mut reqwest::Response,
    idle_timeout: Option<Duration>,
    policy_timing: Option<layered_policy::AttemptPolicyTiming>,
    first_progress_seen: bool,
) -> Result<Result<Option<Bytes>, reqwest::Error>, ChunkWaitTimeout> {
    let now = std::time::Instant::now();
    let mut deadline = idle_timeout
        .and_then(|timeout| now.checked_add(timeout))
        .map(|deadline| (deadline, ChunkWaitTimeout::Idle, 2u8));
    if let Some(policy_timing) = policy_timing {
        if let Some((policy_deadline, phase)) = policy_timing.next_deadline(first_progress_seen) {
            let priority = match phase {
                layered_policy::TimeoutPhase::Total => 0,
                layered_policy::TimeoutPhase::FirstProgress => 1,
            };
            if deadline.is_none_or(|(current, _, current_priority)| {
                (policy_deadline, priority) < (current, current_priority)
            }) {
                deadline = Some((policy_deadline, ChunkWaitTimeout::Policy(phase), priority));
            }
        }
    }

    let Some((deadline, timeout_kind, _)) = deadline else {
        return Ok(resp.chunk().await);
    };
    if deadline <= std::time::Instant::now() {
        return Err(timeout_kind);
    }
    match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), resp.chunk()).await {
        Ok(result) => Ok(result),
        Err(_) => Err(timeout_kind),
    }
}

enum FirstProgressBufferOutcome {
    Ready {
        first_chunk: Option<Bytes>,
        first_byte_ms: Option<u128>,
        progress_seen: bool,
    },
    ReadError(reqwest::Error),
    Timeout(ChunkWaitTimeout),
    ControlLost {
        first_chunk: Bytes,
        first_byte_ms: Option<u128>,
        inspection_unavailable: bool,
    },
}

async fn buffer_until_first_progress(
    resp: &mut reqwest::Response,
    initial_chunk: Option<Bytes>,
    initial_first_byte_ms: Option<u128>,
    request_started: std::time::Instant,
    idle_timeout: Option<Duration>,
    policy_timing: layered_policy::AttemptPolicyTiming,
    declared_sse: bool,
) -> FirstProgressBufferOutcome {
    let mut prefix = Vec::new();
    let mut first_byte_ms = initial_first_byte_ms;
    let mut next = initial_chunk;
    let mut inspector = if declared_sse {
        layered_policy::SseProgressInspector::new()
    } else {
        layered_policy::SseProgressInspector::new_candidate()
    };

    loop {
        let chunk = match next.take() {
            Some(chunk) => Some(chunk),
            None => match read_next_chunk(resp, idle_timeout, Some(policy_timing), false).await {
                Ok(Ok(chunk)) => chunk,
                Ok(Err(err)) => return FirstProgressBufferOutcome::ReadError(err),
                Err(timeout) => return FirstProgressBufferOutcome::Timeout(timeout),
            },
        };
        let Some(chunk) = chunk else {
            let progress_seen = match inspector.finish() {
                Ok(progress) => progress,
                Err(()) => {
                    return FirstProgressBufferOutcome::ControlLost {
                        first_chunk: Bytes::from(prefix),
                        first_byte_ms,
                        inspection_unavailable: true,
                    }
                }
            };
            if let Some(phase) = policy_timing.expired_phase(false) {
                return FirstProgressBufferOutcome::Timeout(ChunkWaitTimeout::Policy(phase));
            }
            return FirstProgressBufferOutcome::Ready {
                first_chunk: (!prefix.is_empty()).then(|| Bytes::from(prefix)),
                first_byte_ms,
                progress_seen,
            };
        };

        if first_byte_ms.is_none() {
            first_byte_ms = Some(request_started.elapsed().as_millis());
        }
        let progress_seen = match inspector.ingest(chunk.as_ref()) {
            Ok(progress) => progress,
            Err(()) => {
                prefix.extend_from_slice(chunk.as_ref());
                return FirstProgressBufferOutcome::ControlLost {
                    first_chunk: Bytes::from(prefix),
                    first_byte_ms,
                    inspection_unavailable: true,
                };
            }
        };
        let exceeds_pre_progress_buffer = chunk.len()
            > layered_policy::RESPONSE_INSPECTION_LIMIT_BYTES.saturating_sub(prefix.len());
        prefix.extend_from_slice(chunk.as_ref());
        if let Some(phase) = policy_timing.expired_phase(false) {
            return FirstProgressBufferOutcome::Timeout(ChunkWaitTimeout::Policy(phase));
        }
        if progress_seen {
            return FirstProgressBufferOutcome::Ready {
                first_chunk: Some(Bytes::from(prefix)),
                first_byte_ms,
                progress_seen: true,
            };
        }
        if exceeds_pre_progress_buffer {
            return FirstProgressBufferOutcome::ControlLost {
                first_chunk: Bytes::from(prefix),
                first_byte_ms,
                inspection_unavailable: false,
            };
        }
    }
}

struct PolicyDeadlineStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    upstream: S,
    timing: layered_policy::AttemptPolicyTiming,
    first_progress_seen: bool,
    inspection_unavailable: bool,
    inspector: Option<layered_policy::SseProgressInspector>,
    special_settings: Arc<Mutex<Vec<serde_json::Value>>>,
    termination: Arc<Mutex<Option<&'static str>>>,
    first_progress_timeout_ms: u32,
    total_timeout_ms: u32,
    deadline_sleep: Option<Pin<Box<tokio::time::Sleep>>>,
    first_write_recorded: bool,
    terminated: bool,
}

impl<S> PolicyDeadlineStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    #[allow(clippy::too_many_arguments)]
    fn new(
        upstream: S,
        timing: layered_policy::AttemptPolicyTiming,
        first_progress_seen: bool,
        inspection_unavailable: bool,
        declared_sse: bool,
        special_settings: Arc<Mutex<Vec<serde_json::Value>>>,
        termination: Arc<Mutex<Option<&'static str>>>,
        first_progress_timeout_ms: u32,
        total_timeout_ms: u32,
    ) -> Self {
        let policy_active = timing.next_deadline(false).is_some();
        let first_progress_seen = first_progress_seen || !policy_active;
        let inspector = (!first_progress_seen && !inspection_unavailable).then(|| {
            if declared_sse {
                layered_policy::SseProgressInspector::new()
            } else {
                layered_policy::SseProgressInspector::new_candidate()
            }
        });
        let deadline_sleep = timing
            .next_deadline(first_progress_seen)
            .map(|(deadline, _)| Box::pin(tokio::time::sleep_until(deadline.into())));
        Self {
            upstream,
            timing,
            first_progress_seen,
            inspection_unavailable,
            inspector,
            special_settings,
            termination,
            first_progress_timeout_ms,
            total_timeout_ms,
            deadline_sleep,
            first_write_recorded: false,
            terminated: false,
        }
    }

    fn reset_deadline_sleep(&mut self) {
        self.deadline_sleep = self
            .timing
            .next_deadline(self.first_progress_seen)
            .map(|(deadline, _)| Box::pin(tokio::time::sleep_until(deadline.into())));
    }

    fn mark_first_progress(&mut self) {
        if self.first_progress_seen {
            return;
        }
        self.first_progress_seen = true;
        self.inspector = None;
        layered_policy::mark_first_progress(&self.special_settings, self.timing);
        self.reset_deadline_sleep();
    }

    fn mark_first_write(&mut self) {
        if self.first_write_recorded {
            return;
        }
        self.first_write_recorded = true;
        layered_policy::mark_client_first_write(&self.special_settings, self.timing);
    }

    fn terminate_for_timeout(&mut self, phase: layered_policy::TimeoutPhase) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        let limit_ms = self.timing.timeout_limit_ms(
            phase,
            self.first_progress_timeout_ms,
            self.total_timeout_ms,
        );
        layered_policy::update_attempt_telemetry(
            &self.special_settings,
            self.timing.sequence,
            serde_json::json!({
                "policyTrigger": format!("{}_timeout", phase.as_str()),
                "policyAction": "disconnect_after_forward",
                "timeoutPhase": phase.as_str(),
                "timeoutLimitMs": limit_ms,
                "timeoutResponseControlLost": true,
                "responseForwardingStarted": true,
                "finalAction": "timeout_disconnected_after_forward",
            }),
        );
        let mut termination = self.termination.lock_or_recover();
        if termination.is_none() {
            *termination = Some(phase.gateway_error_code());
        }
    }

    fn lose_inspection_control(&mut self) {
        if self.inspection_unavailable {
            return;
        }
        self.inspection_unavailable = true;
        self.inspector = None;
        layered_policy::update_attempt_telemetry(
            &self.special_settings,
            self.timing.sequence,
            serde_json::json!({
                "inspectionLimitExceeded": true,
                "inspectionLimitBytes": layered_policy::RESPONSE_INSPECTION_LIMIT_BYTES,
                "timeoutResponseControlLost": true,
                "responseForwardingStarted": true,
            }),
        );
    }

    fn inspect_chunk(&mut self, chunk: &[u8]) {
        if self.first_progress_seen {
            return;
        }
        if self.inspection_unavailable {
            return;
        }
        match self.inspector.as_mut().map(|value| value.ingest(chunk)) {
            Some(Ok(true)) => {
                if let Some(phase) = self.timing.expired_phase(false) {
                    self.terminate_for_timeout(phase);
                } else {
                    self.mark_first_progress();
                }
            }
            Some(Err(())) => self.lose_inspection_control(),
            _ => {}
        }
    }
}

impl<S> Stream for PolicyDeadlineStream<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, reqwest::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        if this.terminated {
            return Poll::Ready(None);
        }
        if let Some(phase) = this.timing.expired_phase(this.first_progress_seen) {
            this.terminate_for_timeout(phase);
            return Poll::Ready(None);
        }

        match Pin::new(&mut this.upstream).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                this.inspect_chunk(chunk.as_ref());
                if this.terminated {
                    return Poll::Ready(None);
                }
                if let Some(phase) = this.timing.expired_phase(this.first_progress_seen) {
                    this.terminate_for_timeout(phase);
                    return Poll::Ready(None);
                }
                this.mark_first_write();
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(err))) => {
                if let Some(phase) = this.timing.expired_phase(this.first_progress_seen) {
                    this.terminate_for_timeout(phase);
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(err)))
                }
            }
            Poll::Ready(None) => {
                if !this.first_progress_seen && !this.inspection_unavailable {
                    match this.inspector.as_mut().map(|value| value.finish()) {
                        Some(Ok(true)) => {
                            if let Some(phase) = this.timing.expired_phase(false) {
                                this.terminate_for_timeout(phase);
                            } else {
                                this.mark_first_progress();
                            }
                        }
                        Some(Err(())) => this.lose_inspection_control(),
                        _ => {}
                    }
                }
                if this.terminated {
                    return Poll::Ready(None);
                }
                if let Some(phase) = this.timing.expired_phase(this.first_progress_seen) {
                    this.terminate_for_timeout(phase);
                }
                Poll::Ready(None)
            }
            Poll::Pending => {
                if let Some(sleep) = this.deadline_sleep.as_mut() {
                    if sleep.as_mut().poll(cx).is_ready() {
                        if let Some(phase) = this.timing.expired_phase(this.first_progress_seen) {
                            this.terminate_for_timeout(phase);
                            return Poll::Ready(None);
                        }
                        this.reset_deadline_sleep();
                    }
                }
                Poll::Pending
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_success_event_stream<R>(
    ctx: CommonCtx<'_, R>,
    provider_ctx: ProviderCtx<'_>,
    attempt_ctx: AttemptCtx<'_>,
    retry_state: &mut attempt_executor::RetryLoopState,
    loop_state: LoopState<'_, R>,
    resp: reqwest::Response,
    status: StatusCode,
    mut response_headers: HeaderMap,
) -> LoopControl
where
    R: tauri::Runtime,
    R::Handle: Unpin,
{
    let common = CommonCtxOwned::from(ctx);
    let provider_ctx_owned = ProviderCtxOwned::from(provider_ctx);

    let started = common.started;
    let upstream_first_byte_timeout_secs = common.upstream_first_byte_timeout_secs;
    let upstream_first_byte_timeout = common.upstream_first_byte_timeout;
    // Per-provider idle timeout overrides the global setting if configured.
    let upstream_stream_idle_timeout = provider_ctx_owned
        .stream_idle_timeout_seconds
        .and_then(|secs| {
            if secs > 0 {
                Some(Duration::from_secs(secs as u64))
            } else {
                None
            }
        })
        .or(common.upstream_stream_idle_timeout);
    let sse_error_retry_count = ctx.sse_error_retry_count;
    let enable_response_fixer = common.enable_response_fixer;
    let response_fixer_stream_config = common.response_fixer_stream_config;

    let provider_id = provider_ctx_owned.provider_id;
    let provider_index = provider_ctx_owned.provider_index;
    let session_reuse = provider_ctx_owned.session_reuse;

    let AttemptCtx {
        attempt_index: _,
        retry_index,
        attempt_started_ms,
        attempt_started,
        policy_timing,
        circuit_before,
        gemini_oauth_response_mode,
        cx2cc_active,
        anthropic_stream_requested,
        ..
    } = attempt_ctx;
    let selection_method = dc::selection_method(provider_index, retry_index, session_reuse);
    let reason_code = dc::success_reason_code(provider_index, retry_index);
    let should_buffer_codex_reasoning_guard = common.codex_reasoning_guard_enabled
        && common.cli_key == "codex"
        && matches!(
            common.forwarded_path.trim_end_matches('/'),
            "/v1/responses" | "/responses"
        );

    let LoopState {
        attempts,
        failed_provider_ids,
        last_outcome,
        circuit_snapshot,
        abort_guard,
    } = loop_state;

    let declared_event_stream = is_event_stream(&response_headers);
    let response_content_type = response_headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let stream_candidate = declared_event_stream
        || (anthropic_stream_requested
            && (response_content_type.is_empty()
                || response_content_type.starts_with("text/plain")));

    if stream_candidate {
        strip_hop_headers(&mut response_headers);
        tracing::info!(
            trace_id = %common.trace_id,
            provider_id,
            cx2cc_active,
            "handling successful upstream event-stream response"
        );
        if cx2cc_active {
            emit_gateway_log(
                &common.state.app,
                "info",
                "CX2CC_SUCCESS_EVENT_STREAM",
                format!(
                    "[CX2CC] handling successful upstream event-stream response trace_id={} provider_id={}",
                    common.trace_id, provider_id
                ),
            );
        }

        let mut resp = resp;

        enum FirstChunkProbe {
            Skipped,
            Ok(Option<Bytes>, Option<u128>),
            ReadError(reqwest::Error),
            Timeout,
            PolicyTimeout(layered_policy::TimeoutPhase),
        }

        let probe = match upstream_first_byte_timeout {
            Some(total) => {
                let elapsed = attempt_started.elapsed();
                if elapsed >= total {
                    FirstChunkProbe::Timeout
                } else {
                    let remaining = total - elapsed;
                    match read_next_chunk(&mut resp, Some(remaining), policy_timing, false).await {
                        Ok(Ok(Some(chunk))) => {
                            FirstChunkProbe::Ok(Some(chunk), Some(started.elapsed().as_millis()))
                        }
                        Ok(Ok(None)) => FirstChunkProbe::Ok(None, None),
                        Ok(Err(err)) => FirstChunkProbe::ReadError(err),
                        Err(ChunkWaitTimeout::Idle) => FirstChunkProbe::Timeout,
                        Err(ChunkWaitTimeout::Policy(phase)) => {
                            FirstChunkProbe::PolicyTimeout(phase)
                        }
                    }
                }
            }
            None => FirstChunkProbe::Skipped,
        };
        let probe_is_empty_event_stream = matches!(probe, FirstChunkProbe::Ok(None, None));

        let mut first_chunk: Option<Bytes> = None;
        let mut initial_first_byte_ms: Option<u128> = None;

        match probe {
            FirstChunkProbe::Ok(chunk, ttfb_ms) => {
                first_chunk = chunk;
                initial_first_byte_ms = ttfb_ms;
            }
            FirstChunkProbe::ReadError(err) => {
                let error_code = GatewayErrorCode::StreamError.as_str();
                let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                let decision = retry.decision;

                let outcome = sse_retry_outcome(
                    format!(
                        "stream_first_chunk_error: category={} code={} decision={} timeout_secs={}",
                        ErrorCategory::SystemError.as_str(),
                        error_code,
                        decision.as_str(),
                        upstream_first_byte_timeout_secs,
                    ),
                    retry,
                );

                return record_system_failure_and_decide(RecordSystemFailureArgs {
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    loop_state: LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    status: Some(status.as_u16()),
                    error_code,
                    decision,
                    outcome,
                    reason: sse_retry_reason(
                        format!("first chunk read error (event-stream): {err}"),
                        retry,
                    ),
                    timeout_secs: Some(upstream_first_byte_timeout_secs),
                })
                .await;
            }
            FirstChunkProbe::Timeout => {
                let error_code = GatewayErrorCode::UpstreamTimeout.as_str();
                let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                let decision = retry.decision;

                let outcome = sse_retry_outcome(
                    format!(
                    "stream_first_byte_timeout: category={} code={} decision={} timeout_secs={}",
                    ErrorCategory::SystemError.as_str(),
                    error_code,
                    decision.as_str(),
                    upstream_first_byte_timeout_secs,
                ),
                    retry,
                );

                return record_system_failure_and_decide(RecordSystemFailureArgs {
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    loop_state: LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    status: Some(status.as_u16()),
                    error_code,
                    decision,
                    outcome,
                    reason: sse_retry_reason("first byte timeout (event-stream)", retry),
                    timeout_secs: Some(upstream_first_byte_timeout_secs),
                })
                .await;
            }
            FirstChunkProbe::PolicyTimeout(phase) => {
                return layered_policy::handle_timeout_before_forward(
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    phase,
                )
                .await;
            }
            FirstChunkProbe::Skipped => {}
        }

        if upstream_first_byte_timeout.is_some()
            && first_chunk.is_none()
            && initial_first_byte_ms.is_none()
            && probe_is_empty_event_stream
        {
            let error_code = GatewayErrorCode::StreamError.as_str();
            let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
            let decision = retry.decision;

            let outcome = sse_retry_outcome(
                format!(
                    "stream_first_chunk_eof: category={} code={} decision={} timeout_secs={}",
                    ErrorCategory::SystemError.as_str(),
                    error_code,
                    decision.as_str(),
                    upstream_first_byte_timeout_secs,
                ),
                retry,
            );

            return record_system_failure_and_decide(RecordSystemFailureArgs {
                ctx,
                provider_ctx,
                attempt_ctx,
                loop_state: LoopState {
                    attempts,
                    failed_provider_ids,
                    last_outcome,
                    circuit_snapshot,
                    abort_guard,
                },
                status: Some(status.as_u16()),
                error_code,
                decision,
                outcome,
                reason: sse_retry_reason("upstream returned empty event-stream", retry),
                timeout_secs: Some(upstream_first_byte_timeout_secs),
            })
            .await;
        }

        let mut stream_first_progress_seen = false;
        let mut stream_inspection_unavailable = false;
        if !should_buffer_codex_reasoning_guard
            && policy_timing.is_some_and(|timing| timing.next_deadline(false).is_some())
        {
            let timing = policy_timing.expect("policy timing checked above");
            match buffer_until_first_progress(
                &mut resp,
                first_chunk.take(),
                initial_first_byte_ms,
                started,
                upstream_stream_idle_timeout,
                timing,
                declared_event_stream,
            )
            .await
            {
                FirstProgressBufferOutcome::Ready {
                    first_chunk: buffered,
                    first_byte_ms,
                    progress_seen,
                } => {
                    first_chunk = buffered;
                    initial_first_byte_ms = first_byte_ms;
                    stream_first_progress_seen = progress_seen;
                    if progress_seen {
                        layered_policy::mark_first_progress(&common.special_settings, timing);
                    }
                }
                FirstProgressBufferOutcome::ControlLost {
                    first_chunk: buffered,
                    first_byte_ms,
                    inspection_unavailable,
                } => {
                    first_chunk = Some(buffered);
                    initial_first_byte_ms = first_byte_ms;
                    stream_inspection_unavailable = inspection_unavailable;
                    layered_policy::update_attempt_telemetry(
                        &common.special_settings,
                        timing.sequence,
                        serde_json::json!({
                            "timeoutResponseControlLost": true,
                            "inspectionLimitExceeded": inspection_unavailable,
                            "inspectionLimitBytes": layered_policy::RESPONSE_INSPECTION_LIMIT_BYTES,
                            "finalAction": "pre_progress_buffer_flushed_control_lost",
                        }),
                    );
                }
                FirstProgressBufferOutcome::ReadError(err) => {
                    let error_code = GatewayErrorCode::StreamError.as_str();
                    let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                    let decision = retry.decision;
                    let outcome = sse_retry_outcome(
                        format!(
                            "stream_pre_progress_read_error: category={} code={} decision={}",
                            ErrorCategory::SystemError.as_str(),
                            error_code,
                            decision.as_str(),
                        ),
                        retry,
                    );
                    return record_system_failure_and_decide(RecordSystemFailureArgs {
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        loop_state: LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        status: Some(status.as_u16()),
                        error_code,
                        decision,
                        outcome,
                        reason: sse_retry_reason(
                            format!("failed while awaiting first meaningful output: {err}"),
                            retry,
                        ),
                        timeout_secs: None,
                    })
                    .await;
                }
                FirstProgressBufferOutcome::Timeout(ChunkWaitTimeout::Policy(phase)) => {
                    return layered_policy::handle_timeout_before_forward(
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        phase,
                    )
                    .await;
                }
                FirstProgressBufferOutcome::Timeout(ChunkWaitTimeout::Idle) => {
                    let error_code = GatewayErrorCode::StreamIdleTimeout.as_str();
                    let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                    let decision = retry.decision;
                    let outcome = sse_retry_outcome(
                        format!(
                            "stream_pre_progress_idle_timeout: category={} code={} decision={}",
                            ErrorCategory::SystemError.as_str(),
                            error_code,
                            decision.as_str(),
                        ),
                        retry,
                    );
                    return record_system_failure_and_decide(RecordSystemFailureArgs {
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        loop_state: LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        status: Some(status.as_u16()),
                        error_code,
                        decision,
                        outcome,
                        reason: sse_retry_reason(
                            "event-stream idle timeout before first meaningful output",
                            retry,
                        ),
                        timeout_secs: upstream_stream_idle_timeout
                            .and_then(|value| u32::try_from(value.as_secs()).ok()),
                    })
                    .await;
                }
            }
        }

        if should_buffer_codex_reasoning_guard {
            let mut raw = Vec::new();
            let mut progress_inspector = if declared_event_stream {
                layered_policy::SseProgressInspector::new()
            } else {
                layered_policy::SseProgressInspector::new_candidate()
            };
            let mut first_progress_seen = false;

            if let Some(chunk) = first_chunk.take() {
                let progress_detected = match progress_inspector.ingest(chunk.as_ref()) {
                    Ok(progress) => progress,
                    Err(()) => {
                        return layered_policy::handle_inspection_limit_before_forward(
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                        )
                        .await;
                    }
                };
                if progress_detected {
                    if let Some(phase) =
                        policy_timing.and_then(|timing| timing.expired_phase(false))
                    {
                        return layered_policy::handle_timeout_before_forward(
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                            phase,
                        )
                        .await;
                    }
                    first_progress_seen = true;
                    if let Some(policy_timing) = policy_timing {
                        layered_policy::mark_first_progress(
                            &common.special_settings,
                            policy_timing,
                        );
                    }
                }
                if let Some(phase) =
                    policy_timing.and_then(|timing| timing.expired_phase(first_progress_seen))
                {
                    return layered_policy::handle_timeout_before_forward(
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        phase,
                    )
                    .await;
                }
                raw.extend_from_slice(chunk.as_ref());
                if raw.len() > MAX_NON_SSE_BODY_BYTES {
                    let error_code = GatewayErrorCode::UpstreamBodyReadError.as_str();
                    let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                    let decision = retry.decision;
                    let outcome = sse_retry_outcome(
                        format!(
                        "stream_buffer_too_large: category={} code={} decision={} limit_bytes={}",
                        ErrorCategory::SystemError.as_str(),
                        error_code,
                        decision.as_str(),
                        MAX_NON_SSE_BODY_BYTES,
                    ),
                        retry,
                    );

                    return record_system_failure_and_decide(RecordSystemFailureArgs {
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        loop_state: LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        status: Some(status.as_u16()),
                        error_code,
                        decision,
                        outcome,
                        reason: sse_retry_reason(
                            format!(
                                "event-stream body exceeded gateway buffer limit ({} bytes)",
                                MAX_NON_SSE_BODY_BYTES
                            ),
                            retry,
                        ),
                        timeout_secs: None,
                    })
                    .await;
                }
            }

            loop {
                let next_chunk = match read_next_chunk(
                    &mut resp,
                    upstream_stream_idle_timeout,
                    policy_timing,
                    first_progress_seen,
                )
                .await
                {
                    Ok(Ok(chunk)) => chunk,
                    Ok(Err(err)) => {
                        let error_code = GatewayErrorCode::StreamError.as_str();
                        let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                        let decision = retry.decision;
                        let outcome = sse_retry_outcome(
                            format!(
                                "stream_buffer_read_error: category={} code={} decision={}",
                                ErrorCategory::SystemError.as_str(),
                                error_code,
                                decision.as_str(),
                            ),
                            retry,
                        );
                        return record_system_failure_and_decide(RecordSystemFailureArgs {
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            loop_state: LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                            status: Some(status.as_u16()),
                            error_code,
                            decision,
                            outcome,
                            reason: sse_retry_reason(
                                format!("failed to buffer event-stream body: {err}"),
                                retry,
                            ),
                            timeout_secs: None,
                        })
                        .await;
                    }
                    Err(ChunkWaitTimeout::Idle) => {
                        let error_code = GatewayErrorCode::UpstreamTimeout.as_str();
                        let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                        let decision = retry.decision;
                        let outcome = sse_retry_outcome(format!(
                            "stream_buffer_idle_timeout: category={} code={} decision={} timeout_secs={}",
                            ErrorCategory::SystemError.as_str(),
                            error_code,
                            decision.as_str(),
                            upstream_stream_idle_timeout
                                .map(|value| value.as_secs())
                                .unwrap_or_default(),
                        ), retry);
                        return record_system_failure_and_decide(RecordSystemFailureArgs {
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            loop_state: LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                            status: Some(status.as_u16()),
                            error_code,
                            decision,
                            outcome,
                            reason: sse_retry_reason(
                                "event-stream idle timeout while buffering",
                                retry,
                            ),
                            timeout_secs: upstream_stream_idle_timeout
                                .and_then(|value| u32::try_from(value.as_secs()).ok()),
                        })
                        .await;
                    }
                    Err(ChunkWaitTimeout::Policy(phase)) => {
                        return layered_policy::handle_timeout_before_forward(
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                            phase,
                        )
                        .await;
                    }
                };

                let Some(chunk) = next_chunk else {
                    break;
                };
                if initial_first_byte_ms.is_none() {
                    initial_first_byte_ms = Some(started.elapsed().as_millis());
                }
                let progress_detected = match progress_inspector.ingest(chunk.as_ref()) {
                    Ok(progress) => progress,
                    Err(()) => {
                        return layered_policy::handle_inspection_limit_before_forward(
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                        )
                        .await;
                    }
                };
                if progress_detected && !first_progress_seen {
                    if let Some(phase) =
                        policy_timing.and_then(|timing| timing.expired_phase(false))
                    {
                        return layered_policy::handle_timeout_before_forward(
                            ctx,
                            provider_ctx,
                            attempt_ctx,
                            LoopState {
                                attempts,
                                failed_provider_ids,
                                last_outcome,
                                circuit_snapshot,
                                abort_guard,
                            },
                            phase,
                        )
                        .await;
                    }
                    first_progress_seen = true;
                    if let Some(policy_timing) = policy_timing {
                        layered_policy::mark_first_progress(
                            &common.special_settings,
                            policy_timing,
                        );
                    }
                }
                if let Some(phase) =
                    policy_timing.and_then(|timing| timing.expired_phase(first_progress_seen))
                {
                    return layered_policy::handle_timeout_before_forward(
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        phase,
                    )
                    .await;
                }
                raw.extend_from_slice(chunk.as_ref());
                if raw.len() > MAX_NON_SSE_BODY_BYTES {
                    let error_code = GatewayErrorCode::UpstreamBodyReadError.as_str();
                    let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                    let decision = retry.decision;
                    let outcome = sse_retry_outcome(
                        format!(
                        "stream_buffer_too_large: category={} code={} decision={} limit_bytes={}",
                        ErrorCategory::SystemError.as_str(),
                        error_code,
                        decision.as_str(),
                        MAX_NON_SSE_BODY_BYTES,
                    ),
                        retry,
                    );

                    return record_system_failure_and_decide(RecordSystemFailureArgs {
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        loop_state: LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        status: Some(status.as_u16()),
                        error_code,
                        decision,
                        outcome,
                        reason: sse_retry_reason(
                            format!(
                                "event-stream body exceeded gateway buffer limit ({} bytes)",
                                MAX_NON_SSE_BODY_BYTES
                            ),
                            retry,
                        ),
                        timeout_secs: None,
                    })
                    .await;
                }
            }

            let progress_detected = match progress_inspector.finish() {
                Ok(progress) => progress,
                Err(()) => {
                    return layered_policy::handle_inspection_limit_before_forward(
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                    )
                    .await;
                }
            };
            if progress_detected && !first_progress_seen {
                if let Some(phase) = policy_timing.and_then(|timing| timing.expired_phase(false)) {
                    return layered_policy::handle_timeout_before_forward(
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        phase,
                    )
                    .await;
                }
                first_progress_seen = true;
                if let Some(policy_timing) = policy_timing {
                    layered_policy::mark_first_progress(&common.special_settings, policy_timing);
                }
            }
            if let Some(phase) =
                policy_timing.and_then(|timing| timing.expired_phase(first_progress_seen))
            {
                return layered_policy::handle_timeout_before_forward(
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    phase,
                )
                .await;
            }

            let raw = if has_gzip_content_encoding(&response_headers) {
                let mut headers_for_decode = response_headers.clone();
                let decoded = maybe_gunzip_response_body_bytes_with_limit(
                    Bytes::from(raw),
                    &mut headers_for_decode,
                    MAX_NON_SSE_BODY_BYTES,
                );
                response_headers = headers_for_decode;
                decoded
            } else {
                Bytes::from(raw)
            };

            let raw =
                if enable_response_fixer && !has_non_identity_content_encoding(&response_headers) {
                    response_headers.remove(header::CONTENT_LENGTH);
                    response_headers.insert(
                        "x-cch-response-fixer",
                        HeaderValue::from_static("processed"),
                    );
                    let fixer_outcome =
                        response_fixer::process_non_stream(raw, response_fixer_stream_config);
                    if let Some(setting) = fixer_outcome.special_setting {
                        response_fixer::push_special_setting(&common.special_settings, setting);
                    }
                    fixer_outcome.body
                } else {
                    raw
                };

            let aggregated = match protocol_bridge::stream::aggregate_responses_event_stream(
                raw.as_ref(),
            ) {
                Ok(value) => value,
                Err(err) => {
                    let overloaded = is_retryable_upstream_overload(&err);
                    let error_code = if overloaded {
                        GatewayErrorCode::UpstreamOverloaded.as_str()
                    } else {
                        GatewayErrorCode::InternalError.as_str()
                    };
                    let category = if overloaded {
                        ErrorCategory::ProviderError
                    } else {
                        ErrorCategory::SystemError
                    };
                    let retry = decide_sse_error_retry(retry_state, sse_error_retry_count);
                    let decision = retry.decision;
                    let outcome = sse_retry_outcome(
                        format!(
                            "codex_event_stream_aggregate_error: category={} code={} decision={} err={err}",
                            category.as_str(),
                            error_code,
                            decision.as_str(),
                        ),
                        retry,
                    );

                    let args = RecordSystemFailureArgs {
                        ctx,
                        provider_ctx,
                        attempt_ctx,
                        loop_state: LoopState {
                            attempts,
                            failed_provider_ids,
                            last_outcome,
                            circuit_snapshot,
                            abort_guard,
                        },
                        status: Some(if overloaded {
                            StatusCode::SERVICE_UNAVAILABLE.as_u16()
                        } else {
                            status.as_u16()
                        }),
                        error_code,
                        decision,
                        outcome,
                        reason: sse_retry_reason(
                            format!("failed to aggregate Codex responses event-stream: {err}"),
                            retry,
                        ),
                        timeout_secs: None,
                    };
                    let control = if overloaded {
                        record_transient_provider_failure_and_decide(args).await
                    } else {
                        record_system_failure_and_decide_no_cooldown(args).await
                    };
                    if overloaded && matches!(control, LoopControl::ContinueRetry) {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    return control;
                }
            };

            let guard_evaluation = codex_reasoning_guard::evaluate_from_json(
                common.cli_key.as_str(),
                common.requested_model.as_deref(),
                &aggregated,
                codex_reasoning_guard::CodexReasoningGuardDetectionOptions {
                    rule_mode: common.codex_reasoning_guard_rule_mode,
                    match_mode: common.codex_reasoning_guard_match_mode,
                    request_kind: common.codex_request_kind,
                    requested_reasoning_effort: common.codex_reasoning_effort.as_deref(),
                    fallback_compare_mode: common.codex_reasoning_guard_compare_mode,
                    fallback_values: common.codex_reasoning_guard_reasoning_equals.as_slice(),
                    model_rules: common.codex_reasoning_guard_model_rules.as_slice(),
                },
            );
            codex_reasoning_guard::push_check_special_setting(
                &common.special_settings,
                provider_id,
                provider_ctx_owned.provider_name_base.as_str(),
                retry_index,
                &guard_evaluation,
            );
            if let Some(matched) = guard_evaluation.matched {
                let mut budget_decision = codex_reasoning_guard::shared_budget_decision(
                    &common.layered_policy_state,
                    common.codex_reasoning_guard_immediate_retry_budget,
                    common.codex_reasoning_guard_delayed_retry_budget,
                    common.codex_reasoning_guard_exhausted_action,
                );
                if let Some(policy_timing) = policy_timing {
                    layered_policy::update_attempt_telemetry(
                        &common.special_settings,
                        policy_timing.sequence,
                        serde_json::json!({
                            "policyTrigger": "reasoning",
                            "policyAction": budget_decision.action_taken,
                            "retryTrigger": matches!(budget_decision.action, codex_reasoning_guard::CodexReasoningGuardBudgetAction::RetrySameProvider).then_some("reasoning"),
                            "retryDelayMs": budget_decision.delay_ms,
                            "retryBudgetUsed": budget_decision.hit_number.min(budget_decision.total_budget),
                            "retryBudgetRemaining": budget_decision.remaining_budget,
                            "finalAction": budget_decision.action_taken,
                        }),
                    );
                }
                let continuation_body = if common.codex_reasoning_guard_stream_action
                    == crate::settings::CodexReasoningGuardStreamAction::ContinuationRecovery
                    && matched.rule_mode
                        == crate::settings::CodexReasoningGuardRuleMode::ReasoningTokens
                    && matches!(
                        budget_decision.action,
                        codex_reasoning_guard::CodexReasoningGuardBudgetAction::RetrySameProvider
                    ) {
                    retry_state
                        .codex_continuation_base_request_body
                        .as_ref()
                        .and_then(|base_body| {
                            codex_reasoning_guard::build_continuation_recovery_body(
                                base_body.as_ref(),
                                common
                                    .codex_reasoning_guard_continuation_marker_text
                                    .as_str(),
                            )
                        })
                } else {
                    None
                };
                if continuation_body.is_some() {
                    budget_decision.action_taken = "continuation_recovery";
                }
                codex_reasoning_guard::push_special_setting(
                    &common.special_settings,
                    provider_id,
                    provider_ctx_owned.provider_name_base.as_str(),
                    retry_index,
                    &matched,
                    budget_decision,
                );
                codex_reasoning_guard::record_guard_retry_attempt(
                    attempts,
                    provider_id,
                    provider_ctx_owned.provider_name_base.as_str(),
                    provider_ctx_owned.provider_base_url_base.as_str(),
                    provider_index,
                    retry_index,
                    session_reuse,
                    attempt_started_ms,
                    attempt_started.elapsed().as_millis(),
                    circuit_before.state.as_str(),
                    circuit_before.failure_count,
                    circuit_before.failure_threshold,
                    &matched,
                    budget_decision,
                );
                if continuation_body.is_some() {
                    let next_count = retry_state
                        .codex_continuation_recovery_count
                        .saturating_add(1);
                    codex_reasoning_guard::push_continuation_recovery_setting(
                        &common.special_settings,
                        provider_id,
                        provider_ctx_owned.provider_name_base.as_str(),
                        retry_index,
                        "continuation_recovery",
                        next_count,
                        retry_state.codex_continuation_recovery_success_count,
                    );
                }
                let outcome = match budget_decision.action {
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::RetrySameProvider => {
                        "codex_reasoning_guard_retry"
                    }
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::ReturnError => {
                        "codex_reasoning_guard_exhausted"
                    }
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::SwitchProvider => {
                        "codex_reasoning_guard_switch_provider"
                    }
                };
                emit_attempt_event_and_log(
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    outcome.to_string(),
                    Some(StatusCode::BAD_GATEWAY.as_u16()),
                    AttemptCircuitFields {
                        state_before: Some(circuit_before.state.as_str()),
                        state_after: Some(circuit_before.state.as_str()),
                        failure_count: Some(circuit_before.failure_count),
                        failure_threshold: Some(circuit_before.failure_threshold),
                    },
                )
                .await;
                match budget_decision.action {
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::RetrySameProvider => {
                        retry_state.allow_next_retry_beyond_max_attempts = true;
                        if let Some(next_body) = continuation_body {
                            retry_state.codex_continuation_request_body =
                                Some(Bytes::from(next_body));
                            retry_state.codex_strip_encrypted_reasoning_response = true;
                            retry_state.codex_continuation_recovery_count = retry_state
                                .codex_continuation_recovery_count
                                .saturating_add(1);
                        }
                        layered_policy::sleep_before_retry(
                            &common.layered_policy_state,
                            Duration::from_millis(budget_decision.delay_ms as u64),
                        )
                        .await;
                        return LoopControl::ContinueRetry;
                    }
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::ReturnError => {
                        *last_outcome = Some(AttemptOutcome::new(
                            ErrorCategory::SystemError.as_str(),
                            codex_reasoning_guard::CODEX_REASONING_GUARD_ERROR_CODE,
                        ));
                        let duration_ms = started.elapsed().as_millis();
                        emit_request_event_and_enqueue_request_log(
                            RequestEndArgs::from_context(RequestEndContextArgs {
                                deps: RequestEndDeps::new(
                                    &common.state.app,
                                    &common.state.db,
                                    &common.state.log_tx,
                                    &common.state.plugin_pipeline,
                                    &common.state.active_requests,
                                ),
                                trace_id: common.trace_id.as_str(),
                                cli_key: common.cli_key.as_str(),
                                method: common.method_hint.as_str(),
                                path: common.forwarded_path.as_str(),
                                observe: common.observe,
                                query: common.query.as_deref(),
                                excluded_from_stats: false,
                                duration_ms,
                                attempts: attempts.as_slice(),
                                special_settings_json: response_fixer::special_settings_json(
                                    &common.special_settings,
                                ),
                                session_id: common.session_id.clone(),
                                requested_model: common.requested_model.clone(),
                                created_at_ms: common.created_at_ms,
                                created_at: common.created_at,
                            })
                            .with_completion(
                                RequestCompletion::failure_with_ttfb(
                                    StatusCode::BAD_GATEWAY.as_u16(),
                                    Some(ErrorCategory::SystemError.as_str()),
                                    codex_reasoning_guard::CODEX_REASONING_GUARD_ERROR_CODE,
                                    initial_first_byte_ms.unwrap_or(duration_ms),
                                ),
                            ),
                        )
                        .await;
                        abort_guard.disarm();
                        return LoopControl::Return(error_response(
                            StatusCode::BAD_GATEWAY,
                            common.trace_id.clone(),
                            codex_reasoning_guard::CODEX_REASONING_GUARD_ERROR_CODE,
                            "Codex reasoning guard retry budget exhausted".to_string(),
                            attempts.clone(),
                        ));
                    }
                    codex_reasoning_guard::CodexReasoningGuardBudgetAction::SwitchProvider => {
                        failed_provider_ids.insert(provider_id);
                        *last_outcome = Some(AttemptOutcome::new(
                            ErrorCategory::SystemError.as_str(),
                            codex_reasoning_guard::CODEX_REASONING_GUARD_ERROR_CODE,
                        ));
                        return LoopControl::BreakRetry;
                    }
                }
            }

            if retry_state.codex_continuation_recovery_count
                > retry_state.codex_continuation_recovery_success_count
            {
                retry_state.codex_continuation_recovery_success_count = retry_state
                    .codex_continuation_recovery_success_count
                    .saturating_add(1);
                codex_reasoning_guard::push_continuation_recovery_setting(
                    &common.special_settings,
                    provider_id,
                    provider_ctx_owned.provider_name_base.as_str(),
                    retry_index,
                    "continuation_recovery_success",
                    retry_state.codex_continuation_recovery_count,
                    retry_state.codex_continuation_recovery_success_count,
                );
            }

            let raw_for_client = if retry_state.codex_strip_encrypted_reasoning_response {
                Bytes::from(codex_reasoning_guard::strip_encrypted_content_from_sse(
                    raw.as_ref(),
                ))
            } else {
                raw
            };
            response_headers.remove(header::CONTENT_LENGTH);
            if let Some(phase) =
                policy_timing.and_then(|timing| timing.expired_phase(first_progress_seen))
            {
                return layered_policy::handle_timeout_before_forward(
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    phase,
                )
                .await;
            }

            let outcome = "success".to_string();
            attempts.push(FailoverAttempt {
                provider_id,
                provider_name: provider_ctx_owned.provider_name_base.clone(),
                base_url: provider_ctx_owned.provider_base_url_base.clone(),
                outcome: outcome.clone(),
                status: Some(status.as_u16()),
                provider_index: Some(provider_index),
                retry_index: Some(retry_index),
                session_reuse,
                error_category: None,
                error_code: None,
                decision: Some("success"),
                reason: None,
                selection_method,
                reason_code: Some(reason_code),
                attempt_started_ms: Some(attempt_started_ms),
                attempt_duration_ms: Some(attempt_started.elapsed().as_millis()),
                circuit_state_before: Some(circuit_before.state.as_str()),
                circuit_state_after: None,
                circuit_failure_count: Some(circuit_before.failure_count),
                circuit_failure_threshold: Some(circuit_before.failure_threshold),
                circuit_recover_at_unix: None,
                circuit_trigger_error_code: None,
                provider_bridged: Some(provider_ctx_owned.provider_bridged),
                timeout_secs: None,
            });

            emit_attempt_event_and_log_with_circuit_before(
                ctx,
                provider_ctx,
                attempt_ctx,
                outcome,
                Some(status.as_u16()),
            )
            .await;

            codex_service_tier::append_result_if_detected(
                common.cli_key.as_str(),
                common.introspection_body.as_slice(),
                Some(raw_for_client.as_ref()),
                &common.special_settings,
            );

            let usage =
                usage::parse_usage_from_json_or_sse_bytes(common.cli_key.as_str(), &raw_for_client);
            let usage_metrics = usage.as_ref().map(|u| u.metrics.clone());
            let requested_model_for_log = common.requested_model.clone().or_else(|| {
                if raw_for_client.is_empty() {
                    None
                } else {
                    usage::parse_model_from_json_or_sse_bytes(
                        common.cli_key.as_str(),
                        &raw_for_client,
                    )
                }
            });

            let now_unix = now_unix_seconds() as i64;
            let change = provider_router::record_success_and_emit_transition(
                provider_router::RecordCircuitArgs::from_state(
                    common.state,
                    common.trace_id.as_str(),
                    common.cli_key.as_str(),
                    provider_id,
                    provider_ctx_owned.provider_name_base.as_str(),
                    provider_ctx_owned.provider_base_url_base.as_str(),
                    now_unix,
                ),
            );
            if let Some(last) = attempts.last_mut() {
                last.circuit_state_after = Some(change.after.state.as_str());
                last.circuit_failure_count = Some(change.after.failure_count);
                last.circuit_failure_threshold = Some(change.after.failure_threshold);
            }
            if let Some(session_id) = common.session_id.as_deref() {
                common.state.session.bind_success(
                    &common.cli_key,
                    session_id,
                    provider_id,
                    common.effective_sort_mode_id,
                    now_unix,
                );
            }

            if let Some(phase) =
                policy_timing.and_then(|timing| timing.expired_phase(first_progress_seen))
            {
                return layered_policy::handle_timeout_before_forward(
                    ctx,
                    provider_ctx,
                    attempt_ctx,
                    LoopState {
                        attempts,
                        failed_provider_ids,
                        last_outcome,
                        circuit_snapshot,
                        abort_guard,
                    },
                    phase,
                )
                .await;
            }
            if let Some(policy_timing) = policy_timing {
                layered_policy::mark_response_forwarding(
                    &common.special_settings,
                    policy_timing.sequence,
                );
                if !raw_for_client.is_empty() {
                    layered_policy::mark_client_first_write(
                        &common.special_settings,
                        policy_timing,
                    );
                }
            }
            let duration_ms = started.elapsed().as_millis();
            emit_request_event_and_enqueue_request_log(
                RequestEndArgs::from_context(RequestEndContextArgs {
                    deps: RequestEndDeps::new(
                        &common.state.app,
                        &common.state.db,
                        &common.state.log_tx,
                        &common.state.plugin_pipeline,
                        &common.state.active_requests,
                    ),
                    trace_id: common.trace_id.as_str(),
                    cli_key: common.cli_key.as_str(),
                    method: common.method_hint.as_str(),
                    path: common.forwarded_path.as_str(),
                    observe: common.observe,
                    query: common.query.as_deref(),
                    excluded_from_stats: false,
                    duration_ms,
                    attempts: attempts.as_slice(),
                    special_settings_json: response_fixer::special_settings_json(
                        &common.special_settings,
                    ),
                    session_id: common.session_id.clone(),
                    requested_model: requested_model_for_log,
                    created_at_ms: common.created_at_ms,
                    created_at: common.created_at,
                })
                .with_completion(RequestCompletion::success(
                    status.as_u16(),
                    initial_first_byte_ms,
                    usage_metrics,
                    None,
                    usage,
                )),
            )
            .await;

            let mut builder = Response::builder().status(status);
            for (k, v) in response_headers.iter() {
                builder = builder.header(k, v);
            }
            builder = builder.header("x-trace-id", common.trace_id.as_str());
            abort_guard.disarm();
            return LoopControl::Return(match builder.body(Body::from(raw_for_client)) {
                Ok(r) => r,
                Err(_) => {
                    let mut fallback = (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        GatewayErrorCode::ResponseBuildError.as_str(),
                    )
                        .into_response();
                    fallback.headers_mut().insert(
                        "x-trace-id",
                        HeaderValue::from_str(common.trace_id.as_str())
                            .unwrap_or(HeaderValue::from_static("unknown")),
                    );
                    fallback
                }
            });
        }

        let outcome = "success".to_string();

        attempts.push(FailoverAttempt {
            provider_id,
            provider_name: provider_ctx_owned.provider_name_base.clone(),
            base_url: provider_ctx_owned.provider_base_url_base.clone(),
            outcome: outcome.clone(),
            status: Some(status.as_u16()),
            provider_index: Some(provider_index),
            retry_index: Some(retry_index),
            session_reuse,
            error_category: None,
            error_code: None,
            decision: Some("success"),
            reason: None,
            selection_method,
            reason_code: Some(reason_code),
            attempt_started_ms: Some(attempt_started_ms),
            attempt_duration_ms: Some(attempt_started.elapsed().as_millis()),
            circuit_state_before: Some(circuit_before.state.as_str()),
            circuit_state_after: None,
            circuit_failure_count: Some(circuit_before.failure_count),
            circuit_failure_threshold: Some(circuit_before.failure_threshold),
            circuit_recover_at_unix: None,
            circuit_trigger_error_code: None,
            provider_bridged: Some(provider_ctx_owned.provider_bridged),
            timeout_secs: None,
        });

        emit_attempt_event_and_log_with_circuit_before(
            ctx,
            provider_ctx,
            attempt_ctx,
            outcome,
            Some(status.as_u16()),
        )
        .await;

        codex_service_tier::append_result_if_detected(
            common.cli_key.as_str(),
            common.introspection_body.as_slice(),
            None,
            &common.special_settings,
        );

        let ctx = build_stream_finalize_ctx(
            &common,
            &provider_ctx_owned,
            attempts.as_slice(),
            status.as_u16(),
            None,
            None,
        );

        let should_gunzip = has_gzip_content_encoding(&response_headers);
        if should_gunzip {
            // 上游可能无视 accept-encoding: identity 返回 gzip；
            response_headers.remove(header::CONTENT_ENCODING);
            response_headers.remove(header::CONTENT_LENGTH);
        }

        let enable_response_fixer_for_this_response =
            enable_response_fixer && !has_non_identity_content_encoding(&response_headers);

        if enable_response_fixer_for_this_response {
            response_headers.remove(header::CONTENT_LENGTH);
            response_headers.insert(
                "x-cch-response-fixer",
                HeaderValue::from_static("processed"),
            );
        }

        let use_sse_relay = common.cli_key == "codex"
            && matches!(
                common.forwarded_path.trim_end_matches('/'),
                "/v1/responses" | "/responses"
            );
        let plugin_pipeline = common.state.plugin_pipeline.clone();
        let plugin_db = common.state.db.clone();
        let trace_id = common.trace_id.clone();
        let stream_policy_timing = policy_timing.expect("attempt policy timing is always present");
        let stream_policy_termination = Arc::new(Mutex::new(None));
        layered_policy::mark_response_forwarding(
            &common.special_settings,
            stream_policy_timing.sequence,
        );

        let body = match (enable_response_fixer_for_this_response, should_gunzip) {
            (true, true) => {
                let upstream =
                    GunzipStream::new(FirstChunkStream::new(first_chunk, resp.bytes_stream()));
                let upstream =
                    gemini_oauth::GeminiOAuthSseStream::new(upstream, gemini_oauth_response_mode);
                let upstream = protocol_bridge::stream::BridgeStream::for_cx2cc(
                    upstream,
                    cx2cc_active,
                    common.requested_model.clone(),
                    common.cx2cc_settings.clone(),
                );
                let upstream = response_fixer::ResponseFixerStream::new(
                    upstream,
                    response_fixer_stream_config,
                    common.special_settings.clone(),
                );
                let upstream = MaybePluginChunkStream::new(
                    upstream,
                    plugin_pipeline.clone(),
                    plugin_db.clone(),
                    trace_id.clone(),
                );
                let upstream = PolicyDeadlineStream::new(
                    upstream,
                    stream_policy_timing,
                    stream_first_progress_seen,
                    stream_inspection_unavailable,
                    declared_event_stream,
                    common.special_settings.clone(),
                    stream_policy_termination.clone(),
                    common.codex_gateway_first_progress_timeout_ms,
                    common.codex_gateway_total_timeout_ms,
                );
                if use_sse_relay {
                    spawn_usage_sse_relay_body(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                        Some(stream_policy_termination.clone()),
                    )
                } else {
                    let stream = UsageSseTeeStream::new(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                    )
                    .with_policy_termination(stream_policy_termination.clone());
                    Body::from_stream(stream)
                }
            }
            (true, false) => {
                let upstream = FirstChunkStream::new(first_chunk, resp.bytes_stream());
                let upstream =
                    gemini_oauth::GeminiOAuthSseStream::new(upstream, gemini_oauth_response_mode);
                let upstream = protocol_bridge::stream::BridgeStream::for_cx2cc(
                    upstream,
                    cx2cc_active,
                    common.requested_model.clone(),
                    common.cx2cc_settings.clone(),
                );
                let upstream = response_fixer::ResponseFixerStream::new(
                    upstream,
                    response_fixer_stream_config,
                    common.special_settings.clone(),
                );
                let upstream = MaybePluginChunkStream::new(
                    upstream,
                    plugin_pipeline.clone(),
                    plugin_db.clone(),
                    trace_id.clone(),
                );
                let upstream = PolicyDeadlineStream::new(
                    upstream,
                    stream_policy_timing,
                    stream_first_progress_seen,
                    stream_inspection_unavailable,
                    declared_event_stream,
                    common.special_settings.clone(),
                    stream_policy_termination.clone(),
                    common.codex_gateway_first_progress_timeout_ms,
                    common.codex_gateway_total_timeout_ms,
                );
                if use_sse_relay {
                    spawn_usage_sse_relay_body(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                        Some(stream_policy_termination.clone()),
                    )
                } else {
                    let stream = UsageSseTeeStream::new(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                    )
                    .with_policy_termination(stream_policy_termination.clone());
                    Body::from_stream(stream)
                }
            }
            (false, true) => {
                let upstream =
                    GunzipStream::new(FirstChunkStream::new(first_chunk, resp.bytes_stream()));
                let upstream =
                    gemini_oauth::GeminiOAuthSseStream::new(upstream, gemini_oauth_response_mode);
                let upstream = protocol_bridge::stream::BridgeStream::for_cx2cc(
                    upstream,
                    cx2cc_active,
                    common.requested_model.clone(),
                    common.cx2cc_settings.clone(),
                );
                let upstream = MaybePluginChunkStream::new(
                    upstream,
                    plugin_pipeline.clone(),
                    plugin_db.clone(),
                    trace_id.clone(),
                );
                let upstream = PolicyDeadlineStream::new(
                    upstream,
                    stream_policy_timing,
                    stream_first_progress_seen,
                    stream_inspection_unavailable,
                    declared_event_stream,
                    common.special_settings.clone(),
                    stream_policy_termination.clone(),
                    common.codex_gateway_first_progress_timeout_ms,
                    common.codex_gateway_total_timeout_ms,
                );
                if use_sse_relay {
                    spawn_usage_sse_relay_body(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                        Some(stream_policy_termination.clone()),
                    )
                } else {
                    let stream = UsageSseTeeStream::new(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                    )
                    .with_policy_termination(stream_policy_termination.clone());
                    Body::from_stream(stream)
                }
            }
            (false, false) => {
                let upstream = FirstChunkStream::new(first_chunk, resp.bytes_stream());
                let upstream =
                    gemini_oauth::GeminiOAuthSseStream::new(upstream, gemini_oauth_response_mode);
                let upstream = protocol_bridge::stream::BridgeStream::for_cx2cc(
                    upstream,
                    cx2cc_active,
                    common.requested_model.clone(),
                    common.cx2cc_settings.clone(),
                );
                let upstream = MaybePluginChunkStream::new(
                    upstream,
                    plugin_pipeline.clone(),
                    plugin_db.clone(),
                    trace_id.clone(),
                );
                let upstream = PolicyDeadlineStream::new(
                    upstream,
                    stream_policy_timing,
                    stream_first_progress_seen,
                    stream_inspection_unavailable,
                    declared_event_stream,
                    common.special_settings.clone(),
                    stream_policy_termination.clone(),
                    common.codex_gateway_first_progress_timeout_ms,
                    common.codex_gateway_total_timeout_ms,
                );
                if use_sse_relay {
                    spawn_usage_sse_relay_body(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                        Some(stream_policy_termination.clone()),
                    )
                } else {
                    let stream = UsageSseTeeStream::new(
                        upstream,
                        ctx,
                        upstream_stream_idle_timeout,
                        initial_first_byte_ms,
                    )
                    .with_policy_termination(stream_policy_termination.clone());
                    Body::from_stream(stream)
                }
            }
        };

        let mut builder = Response::builder().status(status);
        for (k, v) in response_headers.iter() {
            builder = builder.header(k, v);
        }
        builder = builder.header("x-trace-id", common.trace_id.as_str());

        abort_guard.disarm();
        return LoopControl::Return(match builder.body(body) {
            Ok(r) => r,
            Err(_) => {
                let mut fallback = (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    GatewayErrorCode::ResponseBuildError.as_str(),
                )
                    .into_response();
                fallback.headers_mut().insert(
                    "x-trace-id",
                    HeaderValue::from_str(common.trace_id.as_str())
                        .unwrap_or(HeaderValue::from_static("unknown")),
                );
                fallback
            }
        });
    }

    unreachable!("expected event-stream response")
}

fn is_retryable_upstream_overload(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("code=server_is_overloaded") || error.contains("type=service_unavailable_error")
}

#[derive(Debug, Clone, Copy)]
struct SseErrorRetryDecision {
    decision: FailoverDecision,
    retries_used: u32,
    retry_limit: u32,
}

fn decide_sse_error_retry(
    retry_state: &mut attempt_executor::RetryLoopState,
    retry_limit: u32,
) -> SseErrorRetryDecision {
    if retry_state.sse_error_retries_used < retry_limit {
        retry_state.sse_error_retries_used = retry_state.sse_error_retries_used.saturating_add(1);
        retry_state.allow_next_retry_beyond_max_attempts = true;
        return SseErrorRetryDecision {
            decision: FailoverDecision::RetrySameProvider,
            retries_used: retry_state.sse_error_retries_used,
            retry_limit,
        };
    }

    SseErrorRetryDecision {
        decision: FailoverDecision::SwitchProvider,
        retries_used: retry_state.sse_error_retries_used,
        retry_limit,
    }
}

fn sse_retry_outcome(base: impl AsRef<str>, retry: SseErrorRetryDecision) -> String {
    format!(
        "{} sse_retry_used={} sse_retry_limit={}",
        base.as_ref(),
        retry.retries_used,
        retry.retry_limit,
    )
}

fn sse_retry_reason(detail: impl AsRef<str>, retry: SseErrorRetryDecision) -> String {
    let detail = detail.as_ref();
    if matches!(retry.decision, FailoverDecision::RetrySameProvider) {
        return format!(
            "{detail}; retrying current provider after SSE failure ({}/{})",
            retry.retries_used, retry.retry_limit
        );
    }
    if retry.retry_limit == 0 {
        return format!("{detail}; SSE retries disabled, switching provider");
    }
    format!(
        "{detail}; SSE retry limit exhausted ({}/{}), switching provider",
        retry.retries_used, retry.retry_limit
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PendingStream;

    impl Stream for PendingStream {
        type Item = Result<Bytes, reqwest::Error>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    struct SlowOneChunkStream {
        chunk: Option<Bytes>,
        delay: Duration,
    }

    impl Stream for SlowOneChunkStream {
        type Item = Result<Bytes, reqwest::Error>;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let Some(chunk) = self.chunk.take() else {
                return Poll::Ready(None);
            };
            std::thread::sleep(self.delay);
            Poll::Ready(Some(Ok(chunk)))
        }
    }

    async fn next_item<S>(stream: &mut S) -> Option<S::Item>
    where
        S: Stream + Unpin,
    {
        std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)).await
    }

    fn policy_timing(
        first_progress_timeout_ms: u32,
        total_timeout_ms: u32,
    ) -> layered_policy::AttemptPolicyTiming {
        let mut state = layered_policy::LayeredPolicyState::new(
            "codex",
            "/v1/responses",
            true,
            first_progress_timeout_ms,
            total_timeout_ms,
            1,
            0,
            0,
        );
        state.begin_attempt(std::time::Instant::now())
    }

    #[test]
    fn recognizes_explicit_upstream_overload_errors_only() {
        assert!(is_retryable_upstream_overload(
            "Our servers are currently overloaded. type=service_unavailable_error code=server_is_overloaded"
        ));
        assert!(is_retryable_upstream_overload("code=server_is_overloaded"));
        assert!(!is_retryable_upstream_overload(
            "model unsupported type=invalid_request_error code=unsupported_value"
        ));
        assert!(!is_retryable_upstream_overload(
            "invalid utf-8 in SSE frame"
        ));
    }

    #[tokio::test]
    async fn policy_deadline_stream_disconnects_after_first_progress_timeout() {
        let timing = policy_timing(20, 0);
        let special_settings = Arc::new(Mutex::new(Vec::new()));
        layered_policy::record_attempt_dispatch(&special_settings, timing, 7, "Provider", 0);
        let termination = Arc::new(Mutex::new(None));
        let mut stream = PolicyDeadlineStream::new(
            PendingStream,
            timing,
            false,
            false,
            true,
            Arc::clone(&special_settings),
            Arc::clone(&termination),
            20,
            0,
        );

        let item = tokio::time::timeout(Duration::from_millis(250), next_item(&mut stream))
            .await
            .expect("deadline stream should wake");
        assert!(item.is_none());
        assert_eq!(
            *termination.lock().expect("termination"),
            Some(layered_policy::FIRST_PROGRESS_TIMEOUT_ERROR_CODE)
        );
        let settings = special_settings.lock().expect("settings");
        assert_eq!(settings[0]["timeoutPhase"], "first_progress");
        assert_eq!(settings[0]["timeoutResponseControlLost"], true);
        assert_eq!(
            settings[0]["finalAction"],
            "timeout_disconnected_after_forward"
        );
    }

    #[tokio::test]
    async fn policy_deadline_stream_keeps_total_deadline_after_progress() {
        let timing = policy_timing(0, 20);
        let special_settings = Arc::new(Mutex::new(Vec::new()));
        layered_policy::record_attempt_dispatch(&special_settings, timing, 7, "Provider", 0);
        let termination = Arc::new(Mutex::new(None));
        let mut stream = PolicyDeadlineStream::new(
            PendingStream,
            timing,
            true,
            false,
            true,
            Arc::clone(&special_settings),
            Arc::clone(&termination),
            0,
            20,
        );

        let item = tokio::time::timeout(Duration::from_millis(250), next_item(&mut stream))
            .await
            .expect("deadline stream should wake");
        assert!(item.is_none());
        assert_eq!(
            *termination.lock().expect("termination"),
            Some(layered_policy::TOTAL_TIMEOUT_ERROR_CODE)
        );
        let settings = special_settings.lock().expect("settings");
        assert_eq!(settings[0]["timeoutPhase"], "total");
        assert_eq!(settings[0]["timeoutLimitMs"], 20);
    }

    #[tokio::test]
    async fn policy_deadline_rechecks_wall_clock_after_chunk_processing_delay() {
        let timing = policy_timing(5, 0);
        let special_settings = Arc::new(Mutex::new(Vec::new()));
        layered_policy::record_attempt_dispatch(&special_settings, timing, 7, "Provider", 0);
        let termination = Arc::new(Mutex::new(None));
        let mut stream = PolicyDeadlineStream::new(
            SlowOneChunkStream {
                chunk: Some(Bytes::from_static(
                    b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"late\"}\n\n",
                )),
                delay: Duration::from_millis(20),
            },
            timing,
            false,
            false,
            true,
            Arc::clone(&special_settings),
            Arc::clone(&termination),
            5,
            0,
        );

        assert!(next_item(&mut stream).await.is_none());
        assert_eq!(
            *termination.lock().expect("termination"),
            Some(layered_policy::FIRST_PROGRESS_TIMEOUT_ERROR_CODE)
        );
    }

    #[tokio::test]
    async fn policy_deadline_does_not_treat_uninspectable_bytes_as_progress() {
        let timing = policy_timing(1_000, 0);
        let special_settings = Arc::new(Mutex::new(Vec::new()));
        layered_policy::record_attempt_dispatch(&special_settings, timing, 7, "Provider", 0);
        let termination = Arc::new(Mutex::new(None));
        let mut stream = PolicyDeadlineStream::new(
            SlowOneChunkStream {
                chunk: Some(Bytes::from_static(b"reasoning-only bytes")),
                delay: Duration::ZERO,
            },
            timing,
            false,
            true,
            true,
            Arc::clone(&special_settings),
            termination,
            1_000,
            0,
        );

        assert!(next_item(&mut stream).await.is_some());
        let settings = special_settings.lock().expect("settings");
        assert!(settings[0]["firstProgressAtMs"].is_null());
    }

    #[tokio::test]
    async fn policy_deadline_marks_semantic_output_as_progress() {
        let timing = policy_timing(1_000, 0);
        let special_settings = Arc::new(Mutex::new(Vec::new()));
        layered_policy::record_attempt_dispatch(&special_settings, timing, 7, "Provider", 0);
        let termination = Arc::new(Mutex::new(None));
        let mut stream = PolicyDeadlineStream::new(
            SlowOneChunkStream {
                chunk: Some(Bytes::from_static(
                    b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                )),
                delay: Duration::ZERO,
            },
            timing,
            false,
            false,
            true,
            Arc::clone(&special_settings),
            termination,
            1_000,
            0,
        );

        assert!(next_item(&mut stream).await.is_some());
        let settings = special_settings.lock().expect("settings");
        assert!(settings[0]["firstProgressAtMs"].is_number());
    }
}
