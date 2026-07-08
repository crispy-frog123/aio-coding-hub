//! Usage: Stored Codex reasoning analytics samples and summaries.

use crate::db;
use crate::shared::error::{db_err, AppResult};
use crate::shared::time::now_unix_seconds;
use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

const SCHEMA_VERSION: i64 = 2;
const DEFAULT_BACKFILL_LIMIT: u32 = 1000;
const MAX_BACKFILL_LIMIT: u32 = 10_000;
const DEFAULT_RECENT_LIMIT: u32 = 50;
const MAX_RECENT_LIMIT: u32 = 200;
const MAX_IMPORT_BYTES: usize = 16 * 1024 * 1024;
const ANALYSIS_PROFILE_NAME: &str = "516_candidate_review_v1";

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningAnalyticsBackfillInput {
    pub since_created_at_ms: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningAnalyticsSnapshotInput {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub since_created_at_ms: Option<i64>,
    pub recent_limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningAnalyticsImportJsonInput {
    pub source_name: Option<String>,
    pub json_text: String,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum CodexReasoningAnalyticsExportFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningAnalyticsExportInput {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub since_created_at_ms: Option<i64>,
    pub format: CodexReasoningAnalyticsExportFormat,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexReasoningAnalyticsAnalyzeInput {
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub since_created_at_ms: Option<i64>,
    pub reasoning_tokens: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningTokenCount {
    pub value: i64,
    pub count: i64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningGroupedRow {
    pub key: String,
    pub count: i64,
    pub ratio: f64,
    pub final_answer_only_ratio: f64,
    pub commentary_observed_ratio: f64,
    pub avg_duration_total_ms: f64,
    pub avg_output_tps: f64,
    pub top_reasoning_tokens: Vec<CodexReasoningTokenCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningModelFamilyRow {
    pub model_family: String,
    #[serde(flatten)]
    pub row: CodexReasoningGroupedRow,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningEffortRow {
    pub reasoning_effort: String,
    #[serde(flatten)]
    pub row: CodexReasoningGroupedRow,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningFamilyEffortRow {
    pub group_key: String,
    pub group_label: String,
    pub model_family: String,
    pub reasoning_effort: String,
    #[serde(flatten)]
    pub row: CodexReasoningGroupedRow,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningTokenRow {
    pub value: i64,
    pub count: i64,
    pub final_answer_only_ratio: f64,
    pub commentary_observed_ratio: f64,
    pub avg_duration_total_ms: f64,
    pub avg_output_tps: f64,
    pub avg_time_normalization_deviation: f64,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningCandidatePattern {
    pub pattern_key: String,
    pub count: i64,
    pub ratio: f64,
    pub avg_duration_total_ms: f64,
    pub avg_output_tps: f64,
    pub avg_time_normalization_deviation: f64,
    pub last_seen_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningOutputTpsBucket {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsSummary {
    pub total_samples: i64,
    pub continuation_recovery_count: i64,
    pub continuation_recovery_success_count: i64,
    pub continuation_recovery_success_ratio: f64,
    pub final_answer_only_ratio: f64,
    pub commentary_present_ratio: f64,
    pub commentary_observed_ratio: f64,
    pub avg_duration_total_ms: f64,
    pub avg_output_tps: f64,
    pub avg_reasoning_adjusted_tps: f64,
    pub wording: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsSample {
    pub sample_id: String,
    pub gateway_request_id: String,
    pub request_log_id: Option<i64>,
    pub trace_id: Option<String>,
    pub ts: String,
    pub date_key: String,
    pub path: String,
    pub method: String,
    pub request_kind: String,
    pub intercept_exempt_reason: Option<String>,
    pub request_model: Option<String>,
    pub request_model_family: String,
    pub effective_local_model_family: String,
    pub request_reasoning_effort: Option<String>,
    pub input_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub duration_total_ms: Option<i64>,
    pub output_tps: Option<f64>,
    pub reasoning_adjusted_tps: Option<f64>,
    pub time_normalization_deviation: Option<f64>,
    pub final_answer_only: bool,
    pub has_commentary: bool,
    pub commentary_observed: bool,
    pub has_final_answer: bool,
    pub has_tool_call: bool,
    pub has_reasoning_item: bool,
    pub matched_current_rule: bool,
    pub blocked_by_gateway: bool,
    pub internal_retry_attempt_index: Option<i64>,
    pub internal_retry_remaining: Option<i64>,
    pub continuation_recovery_count: i64,
    pub continuation_recovery_success_count: i64,
    pub final_action: String,
    pub upstream_http_status: Option<i64>,
    pub client_http_status: Option<i64>,
    pub source_kind: String,
    pub source_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsSnapshot {
    pub ok: bool,
    pub schema_version: i64,
    pub analytics_ready: bool,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub summary: CodexReasoningAnalyticsSummary,
    pub top_reasoning_tokens: Vec<CodexReasoningTokenCount>,
    pub output_tps_buckets: Vec<CodexReasoningOutputTpsBucket>,
    pub by_model_family: Vec<CodexReasoningModelFamilyRow>,
    pub by_reasoning_effort: Vec<CodexReasoningEffortRow>,
    pub by_model_family_and_effort: Vec<CodexReasoningFamilyEffortRow>,
    pub by_reasoning_token: Vec<CodexReasoningTokenRow>,
    pub candidate_patterns: Vec<CodexReasoningCandidatePattern>,
    pub recent_samples: Vec<CodexReasoningAnalyticsSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsBackfillReport {
    pub scanned: i64,
    pub inserted_or_updated: i64,
    pub skipped: i64,
    pub latest_request_log_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsImportReport {
    pub source_name: String,
    pub imported: i64,
    pub skipped: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalyticsExport {
    pub format: String,
    pub file_name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningFieldCoverage {
    pub reasoning_tokens: f64,
    pub final_answer_only: f64,
    pub commentary_observed: f64,
    pub duration_total_ms: f64,
    pub output_tokens: f64,
    pub model_family: f64,
    pub reasoning_effort: f64,
    pub status: f64,
    pub retry_status: f64,
    pub blocked_status: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningCandidateSummary {
    pub candidate_count: i64,
    pub candidate_ratio: f64,
    pub reasoning_516_count: i64,
    pub final_answer_only_count: i64,
    pub commentary_not_observed_count: i64,
    pub high_time_normalization_deviation_count: i64,
    pub last_seen_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningBaselineComparison {
    pub baseline_count: i64,
    pub candidate_avg_time_normalization_deviation: f64,
    pub baseline_avg_time_normalization_deviation: f64,
    pub candidate_final_answer_only_ratio: f64,
    pub baseline_final_answer_only_ratio: f64,
    pub candidate_commentary_not_observed_ratio: f64,
    pub baseline_commentary_not_observed_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct CodexReasoningAnalysisResult {
    pub ok: bool,
    pub analysis_profile: String,
    pub analysis_value: String,
    pub conclusion: String,
    pub field_coverage: CodexReasoningFieldCoverage,
    pub missing_core_fields: Vec<String>,
    pub decision_reason: String,
    pub sample_count: i64,
    pub candidate_summary: CodexReasoningCandidateSummary,
    pub baseline_comparison: CodexReasoningBaselineComparison,
    pub samples_preview: Vec<CodexReasoningAnalyticsSample>,
}

#[derive(Debug, Clone)]
struct RequestLogAnalyticsRow {
    id: i64,
    trace_id: String,
    method: String,
    path: String,
    special_settings_json: Option<String>,
    requested_model: Option<String>,
    status: Option<i64>,
    error_code: Option<String>,
    duration_ms: i64,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    created_at_ms: i64,
}

fn normalize_limit(value: Option<u32>, default_value: u32, max_value: u32) -> u32 {
    value.unwrap_or(default_value).clamp(1, max_value)
}

fn round_metric(value: f64, decimals: i32) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let factor = 10_f64.powi(decimals);
    (value * factor).round() / factor
}

fn avg_metric<F>(samples: &[CodexReasoningAnalyticsSample], getter: F) -> f64
where
    F: Fn(&CodexReasoningAnalyticsSample) -> Option<f64>,
{
    let mut sum = 0.0;
    let mut count = 0.0;
    for sample in samples {
        if let Some(value) = getter(sample).filter(|value| value.is_finite()) {
            sum += value;
            count += 1.0;
        }
    }
    if count == 0.0 {
        0.0
    } else {
        sum / count
    }
}

fn iso_from_ms(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

fn date_key_from_ms(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .format("%Y-%m-%d")
        .to_string()
}

fn validate_date_key(value: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() == 10
        && trimmed.as_bytes()[4] == b'-'
        && trimmed.as_bytes()[7] == b'-'
        && trimmed
            .chars()
            .enumerate()
            .all(|(index, ch)| index == 4 || index == 7 || ch.is_ascii_digit())
    {
        return Ok(Some(trimmed.to_string()));
    }
    Err("SEC_INVALID_INPUT: invalid date key".into())
}

fn normalize_model_family(model: Option<&str>) -> String {
    let model = model.unwrap_or("").trim().to_lowercase();
    if model.is_empty() {
        return "unknown".to_string();
    }
    for suffix in [
        "-xhigh",
        "-high",
        "-medium",
        "-low",
        "-minimal",
        "-none",
        "-thinking",
        "-reasoning",
    ] {
        if let Some(stripped) = model.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    model
}

fn known_model_default_effort(model: Option<&str>) -> Option<String> {
    match model.unwrap_or("").trim().to_lowercase().as_str() {
        "gpt-5.5" => Some("medium".to_string()),
        "gpt-5.5-pro" => Some("high".to_string()),
        "gpt-5.4" | "gpt-5.4-mini" | "gpt-5.4-nano" => Some("none".to_string()),
        "gpt-5.4-pro" => Some("medium".to_string()),
        _ => None,
    }
}

fn normalize_effort(value: Option<&str>) -> Option<String> {
    let raw = value?.trim().to_lowercase();
    match raw.as_str() {
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh" => Some(raw),
        _ => None,
    }
}

fn read_string(value: Option<&Value>) -> Option<String> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn read_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn read_bool(value: Option<&Value>) -> Option<bool> {
    match value? {
        Value::Bool(value) => Some(*value),
        Value::Number(number) => Some(number.as_i64()? != 0),
        Value::String(text) => match text.trim().to_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn parse_special_settings(raw: Option<&str>) -> Vec<Value> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(items)) => items,
        Ok(value @ Value::Object(_)) => vec![value],
        _ => Vec::new(),
    }
}

fn latest_setting<'a>(settings: &'a [Value], setting_type: &str) -> Option<&'a Value> {
    settings.iter().rev().find(|setting| {
        setting
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|value| value == setting_type)
    })
}

fn count_guard_settings(settings: &[Value]) -> i64 {
    settings
        .iter()
        .filter(|setting| {
            setting
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "codex_reasoning_guard")
        })
        .count() as i64
}

fn derive_final_action(row: &RequestLogAnalyticsRow, guard: Option<&Value>) -> String {
    if let Some(action) = read_string(guard.and_then(|value| value.get("actionTaken")))
        .or_else(|| read_string(guard.and_then(|value| value.get("action"))))
    {
        return action;
    }
    if row.error_code.as_deref() == Some("GW_CODEX_REASONING_GUARD") {
        return "return_guard_error_no_circuit".to_string();
    }
    if row.error_code.is_some() {
        return "failed".to_string();
    }
    if matches!(row.status, Some(status) if (200..400).contains(&status)) {
        return "success".to_string();
    }
    "completed".to_string()
}

fn row_to_request_log_analytics(
    row: &rusqlite::Row<'_>,
) -> Result<RequestLogAnalyticsRow, rusqlite::Error> {
    Ok(RequestLogAnalyticsRow {
        id: row.get(0)?,
        trace_id: row.get(1)?,
        method: row.get(2)?,
        path: row.get(3)?,
        special_settings_json: row.get(4)?,
        requested_model: row.get(5)?,
        status: row.get(6)?,
        error_code: row.get(7)?,
        duration_ms: row.get(8)?,
        input_tokens: row.get(9)?,
        output_tokens: row.get(10)?,
        total_tokens: row.get(11)?,
        created_at_ms: row.get(12)?,
    })
}

fn build_request_log_sample(row: RequestLogAnalyticsRow) -> CodexReasoningAnalyticsSample {
    let settings = parse_special_settings(row.special_settings_json.as_deref());
    let guard = latest_setting(&settings, "codex_reasoning_guard");
    let continuation_recovery = latest_setting(&settings, "codex_continuation_recovery");
    let effort_setting = latest_setting(&settings, "codex_reasoning_effort");
    let reasoning_effort = effort_setting
        .and_then(|setting| normalize_effort(read_string(setting.get("effort")).as_deref()))
        .or_else(|| known_model_default_effort(row.requested_model.as_deref()));
    let reasoning_tokens = read_i64(guard.and_then(|value| value.get("reasoningTokens")));
    let final_answer_only = read_bool(guard.and_then(|value| value.get("finalAnswerOnly")));
    let commentary_observed = read_bool(guard.and_then(|value| value.get("commentaryObserved")));
    let has_tool_call = read_bool(guard.and_then(|value| value.get("hasToolCall")));
    let has_reasoning_item = read_bool(guard.and_then(|value| value.get("hasReasoningItem")));
    let request_kind = read_string(guard.and_then(|value| value.get("requestKind")))
        .unwrap_or_else(|| "turn".to_string());
    let intercept_exempt_reason =
        read_string(guard.and_then(|value| value.get("interceptExemptReason")));
    let guard_hit_count = count_guard_settings(&settings);
    let matched_current_rule = guard_hit_count > 0 && intercept_exempt_reason.is_none();
    let blocked_by_gateway = row.error_code.as_deref() == Some("GW_CODEX_REASONING_GUARD");
    let output_tokens = row.output_tokens.unwrap_or(0);
    let observed_tokens = row
        .total_tokens
        .or_else(|| reasoning_tokens.map(|tokens| tokens + output_tokens))
        .unwrap_or(output_tokens);
    let duration_ms = row.duration_ms.max(0);
    let output_tps = (duration_ms > 0)
        .then(|| round_metric((output_tokens as f64 * 1000.0) / duration_ms as f64, 4));
    let reasoning_adjusted_tps = (duration_ms >= 250).then(|| {
        round_metric(
            ((reasoning_tokens.unwrap_or(0) + output_tokens) as f64 * 1000.0) / duration_ms as f64,
            4,
        )
    });
    let time_normalization_deviation = if duration_ms > 0 && observed_tokens > 0 {
        let ms_per_token = duration_ms as f64 / observed_tokens as f64;
        Some(round_metric(((35.0 - ms_per_token) / 35.0).max(0.0), 4))
    } else {
        None
    };
    let model_family = normalize_model_family(row.requested_model.as_deref());
    let final_action = derive_final_action(&row, guard);
    let continuation_recovery_count =
        read_i64(continuation_recovery.and_then(|value| value.get("continuationRecoveryCount")))
            .unwrap_or(0);
    let continuation_recovery_success_count = read_i64(
        continuation_recovery.and_then(|value| value.get("continuationRecoverySuccessCount")),
    )
    .unwrap_or(0);

    CodexReasoningAnalyticsSample {
        sample_id: format!("request_log:{}", row.id),
        gateway_request_id: row.trace_id.clone(),
        request_log_id: Some(row.id),
        trace_id: Some(row.trace_id),
        ts: iso_from_ms(row.created_at_ms),
        date_key: date_key_from_ms(row.created_at_ms),
        path: row.path,
        method: row.method,
        request_kind,
        intercept_exempt_reason,
        request_model: row.requested_model,
        request_model_family: model_family.clone(),
        effective_local_model_family: model_family,
        request_reasoning_effort: reasoning_effort,
        input_tokens: row.input_tokens,
        reasoning_tokens,
        output_tokens: row.output_tokens,
        total_tokens: row.total_tokens,
        duration_total_ms: Some(duration_ms),
        output_tps,
        reasoning_adjusted_tps,
        time_normalization_deviation,
        final_answer_only: final_answer_only.unwrap_or(false),
        has_commentary: commentary_observed.unwrap_or(false),
        commentary_observed: commentary_observed.unwrap_or(false),
        has_final_answer: final_answer_only.unwrap_or(false),
        has_tool_call: has_tool_call.unwrap_or(false),
        has_reasoning_item: has_reasoning_item.unwrap_or(false),
        matched_current_rule,
        blocked_by_gateway,
        internal_retry_attempt_index: None,
        internal_retry_remaining: read_i64(
            guard.and_then(|value| value.get("guardBudgetRemaining")),
        ),
        continuation_recovery_count,
        continuation_recovery_success_count,
        final_action,
        upstream_http_status: row.status,
        client_http_status: row.status,
        source_kind: "request_log".to_string(),
        source_name: Some("AIO request_logs".to_string()),
    }
}

fn sample_from_value(
    value: &Value,
    source_name: &str,
    index: usize,
) -> Option<CodexReasoningAnalyticsSample> {
    let obj = value.as_object()?;
    let created_at_ms = read_i64(obj.get("request_started_at_ms"))
        .or_else(|| read_i64(obj.get("created_at_ms")))
        .or_else(|| read_i64(obj.get("ts_ms")))
        .unwrap_or_else(|| now_unix_seconds() * 1000);
    let ts = read_string(obj.get("ts")).unwrap_or_else(|| iso_from_ms(created_at_ms));
    let date_key = read_string(obj.get("date_key"))
        .or_else(|| ts.get(0..10).map(str::to_string))
        .unwrap_or_else(|| date_key_from_ms(created_at_ms));
    let request_model = read_string(obj.get("request_model"))
        .or_else(|| read_string(obj.get("effective_local_model")))
        .or_else(|| read_string(obj.get("model")));
    let model_family = read_string(obj.get("effective_local_model_family"))
        .or_else(|| read_string(obj.get("request_model_family")))
        .unwrap_or_else(|| normalize_model_family(request_model.as_deref()));
    let sample_id = read_string(obj.get("sample_id"))
        .or_else(|| read_string(obj.get("attempt_id")))
        .unwrap_or_else(|| format!("import:{source_name}:{index}"));
    let gateway_request_id = read_string(obj.get("gateway_request_id"))
        .or_else(|| read_string(obj.get("request_id")))
        .unwrap_or_else(|| sample_id.clone());
    let commentary_observed = read_bool(obj.get("commentary_observed"))
        .or_else(|| read_bool(obj.get("has_commentary")))
        .unwrap_or(false);
    Some(CodexReasoningAnalyticsSample {
        sample_id,
        gateway_request_id,
        request_log_id: read_i64(obj.get("request_log_id")),
        trace_id: read_string(obj.get("trace_id")),
        ts,
        date_key,
        path: read_string(obj.get("path")).unwrap_or_else(|| "/v1/responses".to_string()),
        method: read_string(obj.get("method")).unwrap_or_else(|| "POST".to_string()),
        request_kind: read_string(obj.get("request_kind")).unwrap_or_else(|| "turn".to_string()),
        intercept_exempt_reason: read_string(obj.get("intercept_exempt_reason")),
        request_model,
        request_model_family: model_family.clone(),
        effective_local_model_family: model_family,
        request_reasoning_effort: read_string(obj.get("request_reasoning_effort"))
            .or_else(|| read_string(obj.get("reasoning_effort"))),
        input_tokens: read_i64(obj.get("input_tokens")),
        reasoning_tokens: read_i64(obj.get("reasoning_tokens")),
        output_tokens: read_i64(obj.get("output_tokens")),
        total_tokens: read_i64(obj.get("total_tokens")),
        duration_total_ms: read_i64(obj.get("duration_total_ms")),
        output_tps: read_f64(obj.get("output_tps")),
        reasoning_adjusted_tps: read_f64(obj.get("reasoning_adjusted_tps")),
        time_normalization_deviation: read_f64(obj.get("time_normalization_deviation")),
        final_answer_only: read_bool(obj.get("final_answer_only")).unwrap_or(false),
        has_commentary: read_bool(obj.get("has_commentary")).unwrap_or(commentary_observed),
        commentary_observed,
        has_final_answer: read_bool(obj.get("has_final_answer")).unwrap_or(false),
        has_tool_call: read_bool(obj.get("has_tool_call")).unwrap_or(false),
        has_reasoning_item: read_bool(obj.get("has_reasoning_item")).unwrap_or(false),
        matched_current_rule: read_bool(obj.get("matched_current_rule")).unwrap_or(false),
        blocked_by_gateway: read_bool(obj.get("blocked_by_gateway")).unwrap_or(false),
        internal_retry_attempt_index: read_i64(obj.get("internal_retry_attempt_index")),
        internal_retry_remaining: read_i64(obj.get("internal_retry_remaining")),
        continuation_recovery_count: read_i64(obj.get("continuation_recovery_count")).unwrap_or(0),
        continuation_recovery_success_count: read_i64(
            obj.get("continuation_recovery_success_count"),
        )
        .unwrap_or(0),
        final_action: read_string(obj.get("final_action"))
            .unwrap_or_else(|| "imported".to_string()),
        upstream_http_status: read_i64(obj.get("upstream_http_status")),
        client_http_status: read_i64(obj.get("client_http_status")),
        source_kind: "historical_import".to_string(),
        source_name: Some(source_name.to_string()),
    })
}

fn extract_import_samples(root: &Value) -> Vec<Value> {
    if let Value::Array(items) = root {
        return items.clone();
    }
    for pointer in [
        "/samples",
        "/recent_samples",
        "/data/samples",
        "/summary/samples",
    ] {
        if let Some(Value::Array(items)) = root.pointer(pointer) {
            return items.clone();
        }
    }
    Vec::new()
}

fn store_sample(
    conn: &rusqlite::Connection,
    sample: &CodexReasoningAnalyticsSample,
) -> AppResult<bool> {
    let now = now_unix_seconds();
    let sample_json = serde_json::to_string(sample)
        .map_err(|e| db_err!("failed to encode reasoning analytics sample: {e}"))?;
    let affected = conn
        .execute(
            r#"
INSERT INTO codex_reasoning_analytics_samples (
  sample_key, source_kind, source_name, request_log_id, trace_id, date_key, created_at_ms,
  sample_json, request_model, model_family, reasoning_effort, reasoning_tokens,
  final_answer_only, commentary_observed, has_tool_call, has_reasoning_item,
  matched_current_rule, blocked_by_gateway, client_http_status, duration_total_ms, updated_at
) VALUES (
  ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
)
ON CONFLICT(sample_key) DO UPDATE SET
  source_kind = excluded.source_kind,
  source_name = excluded.source_name,
  request_log_id = excluded.request_log_id,
  trace_id = excluded.trace_id,
  date_key = excluded.date_key,
  created_at_ms = excluded.created_at_ms,
  sample_json = excluded.sample_json,
  request_model = excluded.request_model,
  model_family = excluded.model_family,
  reasoning_effort = excluded.reasoning_effort,
  reasoning_tokens = excluded.reasoning_tokens,
  final_answer_only = excluded.final_answer_only,
  commentary_observed = excluded.commentary_observed,
  has_tool_call = excluded.has_tool_call,
  has_reasoning_item = excluded.has_reasoning_item,
  matched_current_rule = excluded.matched_current_rule,
  blocked_by_gateway = excluded.blocked_by_gateway,
  client_http_status = excluded.client_http_status,
  duration_total_ms = excluded.duration_total_ms,
  updated_at = excluded.updated_at
"#,
            params![
                sample.sample_id,
                sample.source_kind,
                sample.source_name,
                sample.request_log_id,
                sample.trace_id,
                sample.date_key,
                DateTime::parse_from_rfc3339(&sample.ts)
                    .map(|dt| dt.timestamp_millis())
                    .unwrap_or_else(|_| now * 1000),
                sample_json,
                sample.request_model,
                sample.effective_local_model_family,
                sample.request_reasoning_effort,
                sample.reasoning_tokens,
                if sample.final_answer_only { 1 } else { 0 },
                if sample.commentary_observed { 1 } else { 0 },
                if sample.has_tool_call { 1 } else { 0 },
                if sample.has_reasoning_item { 1 } else { 0 },
                if sample.matched_current_rule { 1 } else { 0 },
                if sample.blocked_by_gateway { 1 } else { 0 },
                sample.client_http_status,
                sample.duration_total_ms,
                now,
            ],
        )
        .map_err(|e| db_err!("failed to store reasoning analytics sample: {e}"))?;
    Ok(affected > 0)
}

pub fn backfill_from_request_logs(
    db: &db::Db,
    input: CodexReasoningAnalyticsBackfillInput,
) -> AppResult<CodexReasoningAnalyticsBackfillReport> {
    if matches!(input.since_created_at_ms, Some(value) if value <= 0) {
        return Err("SEC_INVALID_INPUT: invalid sinceCreatedAtMs".into());
    }
    let limit = normalize_limit(input.limit, DEFAULT_BACKFILL_LIMIT, MAX_BACKFILL_LIMIT);
    let mut conn = db.open_connection()?;
    let sql = if input.since_created_at_ms.is_some() {
        r#"
SELECT id, trace_id, method, path, special_settings_json, requested_model, status, error_code,
       duration_ms, input_tokens, output_tokens, total_tokens, created_at_ms
FROM request_logs
WHERE cli_key = 'codex' AND created_at_ms >= ?1
ORDER BY id DESC
LIMIT ?2
"#
    } else {
        r#"
SELECT id, trace_id, method, path, special_settings_json, requested_model, status, error_code,
       duration_ms, input_tokens, output_tokens, total_tokens, created_at_ms
FROM request_logs
WHERE cli_key = 'codex'
ORDER BY id DESC
LIMIT ?1
"#
    };
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| db_err!("failed to prepare codex request log backfill query: {e}"))?;
    let rows = if let Some(since_ms) = input.since_created_at_ms {
        stmt.query_map(params![since_ms, limit], row_to_request_log_analytics)
    } else {
        stmt.query_map(params![limit], row_to_request_log_analytics)
    }
    .map_err(|e| db_err!("failed to query codex request logs for analytics: {e}"))?;

    let mut request_rows = Vec::new();
    for row in rows {
        request_rows.push(row.map_err(|e| db_err!("failed to read request log row: {e}"))?);
    }
    drop(stmt);

    let latest_request_log_id = request_rows.iter().map(|row| row.id).max();
    let tx = conn
        .transaction()
        .map_err(|e| db_err!("failed to start reasoning analytics backfill transaction: {e}"))?;
    let mut inserted_or_updated = 0;
    let mut skipped = 0;
    for row in &request_rows {
        let sample = build_request_log_sample(row.clone());
        if store_sample(&tx, &sample)? {
            inserted_or_updated += 1;
        } else {
            skipped += 1;
        }
    }
    tx.commit()
        .map_err(|e| db_err!("failed to commit reasoning analytics backfill: {e}"))?;

    Ok(CodexReasoningAnalyticsBackfillReport {
        scanned: request_rows.len() as i64,
        inserted_or_updated,
        skipped,
        latest_request_log_id,
    })
}

pub fn import_json(
    db: &db::Db,
    input: CodexReasoningAnalyticsImportJsonInput,
) -> AppResult<CodexReasoningAnalyticsImportReport> {
    if input.json_text.len() > MAX_IMPORT_BYTES {
        return Err("SEC_INVALID_INPUT: import json is too large".into());
    }
    let source_name = input
        .source_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("gateway-json-import")
        .chars()
        .take(120)
        .collect::<String>();
    let root: Value = serde_json::from_str(&input.json_text)
        .map_err(|e| db_err!("failed to parse reasoning analytics import json: {e}"))?;
    let raw_samples = extract_import_samples(&root);
    let mut conn = db.open_connection()?;
    let tx = conn
        .transaction()
        .map_err(|e| db_err!("failed to start reasoning analytics import transaction: {e}"))?;
    let mut imported = 0;
    let mut skipped = 0;
    for (index, value) in raw_samples.iter().enumerate() {
        if let Some(sample) = sample_from_value(value, &source_name, index) {
            store_sample(&tx, &sample)?;
            imported += 1;
        } else {
            skipped += 1;
        }
    }
    tx.commit()
        .map_err(|e| db_err!("failed to commit reasoning analytics import: {e}"))?;
    Ok(CodexReasoningAnalyticsImportReport {
        source_name,
        imported,
        skipped,
    })
}

fn load_samples(
    db: &db::Db,
    date_from: Option<&str>,
    date_to: Option<&str>,
    since_created_at_ms: Option<i64>,
    limit: Option<u32>,
) -> AppResult<Vec<CodexReasoningAnalyticsSample>> {
    let date_from = validate_date_key(date_from)?;
    let date_to = validate_date_key(date_to)?;
    let since_created_at_ms =
        since_created_at_ms.filter(|value| *value > 0 && *value < 4_102_444_800_000);
    if let (Some(from), Some(to)) = (&date_from, &date_to) {
        if to < from {
            return Err("SEC_INVALID_INPUT: date_to must be after date_from".into());
        }
    }
    let conn = db.open_connection()?;
    let mut conditions = Vec::new();
    let mut date_values = Vec::new();
    if let Some(from) = &date_from {
        conditions.push("date_key >= ?");
        date_values.push(from.clone());
    }
    if let Some(to) = &date_to {
        conditions.push("date_key <= ?");
        date_values.push(to.clone());
    }
    if since_created_at_ms.is_some() {
        conditions.push("created_at_ms >= ?");
    }
    let where_sql = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    let limit_sql = limit.map(|limit| format!(" LIMIT {}", limit.min(MAX_BACKFILL_LIMIT)));
    let sql = format!(
        "SELECT sample_json FROM codex_reasoning_analytics_samples {where_sql} ORDER BY created_at_ms DESC, id DESC{}",
        limit_sql.unwrap_or_default()
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| db_err!("failed to prepare reasoning analytics sample query: {e}"))?;
    let mut values: Vec<&dyn rusqlite::ToSql> = Vec::new();
    for value in &date_values {
        values.push(value);
    }
    if let Some(value) = &since_created_at_ms {
        values.push(value);
    }
    let rows = stmt
        .query_map(values.as_slice(), |row| row.get::<_, String>(0))
        .map_err(|e| db_err!("failed to query reasoning analytics samples: {e}"))?;
    let mut samples = Vec::new();
    for row in rows {
        let raw = row.map_err(|e| db_err!("failed to read reasoning analytics row: {e}"))?;
        if let Ok(sample) = serde_json::from_str::<CodexReasoningAnalyticsSample>(&raw) {
            samples.push(sample);
        }
    }
    Ok(samples)
}

fn top_reasoning_tokens(
    samples: &[CodexReasoningAnalyticsSample],
    limit: usize,
) -> Vec<CodexReasoningTokenCount> {
    let mut counts: BTreeMap<i64, i64> = BTreeMap::new();
    for sample in samples {
        if let Some(tokens) = sample.reasoning_tokens {
            *counts.entry(tokens).or_default() += 1;
        }
    }
    let total = samples.len().max(1) as f64;
    let mut rows = counts
        .into_iter()
        .map(|(value, count)| CodexReasoningTokenCount {
            value,
            count,
            ratio: round_metric(count as f64 / total, 6),
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then(left.value.cmp(&right.value))
    });
    rows.truncate(limit);
    rows
}

fn repeated_reasoning_tokens(
    samples: &[CodexReasoningAnalyticsSample],
    limit: usize,
) -> Vec<CodexReasoningTokenCount> {
    let mut rows = top_reasoning_tokens(samples, samples.len());
    rows.retain(|row| row.count > 1);
    rows.truncate(limit);
    rows
}

fn grouped_row(
    key: String,
    group: &[CodexReasoningAnalyticsSample],
    total: usize,
) -> CodexReasoningGroupedRow {
    let count = group.len() as i64;
    CodexReasoningGroupedRow {
        key,
        count,
        ratio: if total == 0 {
            0.0
        } else {
            round_metric(count as f64 / total as f64, 6)
        },
        final_answer_only_ratio: if count == 0 {
            0.0
        } else {
            round_metric(
                group
                    .iter()
                    .filter(|sample| sample.final_answer_only)
                    .count() as f64
                    / count as f64,
                6,
            )
        },
        commentary_observed_ratio: if count == 0 {
            0.0
        } else {
            round_metric(
                group
                    .iter()
                    .filter(|sample| sample.commentary_observed)
                    .count() as f64
                    / count as f64,
                6,
            )
        },
        avg_duration_total_ms: round_metric(
            avg_metric(group, |sample| {
                sample.duration_total_ms.map(|value| value as f64)
            }),
            2,
        ),
        avg_output_tps: round_metric(avg_metric(group, |sample| sample.output_tps), 4),
        top_reasoning_tokens: repeated_reasoning_tokens(group, 3),
    }
}

fn group_samples<F>(
    samples: &[CodexReasoningAnalyticsSample],
    key_fn: F,
) -> Vec<CodexReasoningGroupedRow>
where
    F: Fn(&CodexReasoningAnalyticsSample) -> String,
{
    let mut groups: HashMap<String, Vec<CodexReasoningAnalyticsSample>> = HashMap::new();
    for sample in samples {
        groups
            .entry(key_fn(sample))
            .or_default()
            .push(sample.clone());
    }
    let mut rows = groups
        .into_iter()
        .map(|(key, group)| grouped_row(key, &group, samples.len()))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.count.cmp(&left.count).then(left.key.cmp(&right.key)));
    rows
}

fn output_tps_buckets(
    samples: &[CodexReasoningAnalyticsSample],
) -> Vec<CodexReasoningOutputTpsBucket> {
    let mut buckets = vec![
        ("0-5".to_string(), 0),
        ("5-15".to_string(), 0),
        ("15-30".to_string(), 0),
        ("30+".to_string(), 0),
    ];
    for value in samples.iter().filter_map(|sample| sample.output_tps) {
        let index = if value < 5.0 {
            0
        } else if value < 15.0 {
            1
        } else if value < 30.0 {
            2
        } else {
            3
        };
        buckets[index].1 += 1;
    }
    buckets
        .into_iter()
        .map(|(label, count)| CodexReasoningOutputTpsBucket { label, count })
        .collect()
}

fn candidate_patterns(
    samples: &[CodexReasoningAnalyticsSample],
) -> Vec<CodexReasoningCandidatePattern> {
    let mut groups: HashMap<String, Vec<CodexReasoningAnalyticsSample>> = HashMap::new();
    for sample in samples {
        if let Some(tokens) = sample.reasoning_tokens {
            if sample.final_answer_only && !sample.commentary_observed {
                let key = format!("reasoning={tokens}|final_answer_only|commentary_not_observed");
                groups.entry(key).or_default().push(sample.clone());
            }
        }
    }
    let total = samples.len().max(1) as f64;
    let mut rows = groups
        .into_iter()
        .map(|(pattern_key, group)| {
            let last_seen_at = group.iter().map(|sample| sample.ts.clone()).max();
            let status = if group.iter().any(|sample| sample.blocked_by_gateway) {
                "blocked"
            } else if group.iter().any(|sample| sample.matched_current_rule) {
                "matched_current_rule"
            } else if group.iter().all(|sample| {
                sample.intercept_exempt_reason.as_deref() == Some("context_compaction")
            }) {
                "context_compaction_exempt"
            } else {
                "observe_only"
            };
            CodexReasoningCandidatePattern {
                pattern_key,
                count: group.len() as i64,
                ratio: round_metric(group.len() as f64 / total, 6),
                avg_duration_total_ms: round_metric(
                    avg_metric(&group, |sample| {
                        sample.duration_total_ms.map(|value| value as f64)
                    }),
                    2,
                ),
                avg_output_tps: round_metric(avg_metric(&group, |sample| sample.output_tps), 4),
                avg_time_normalization_deviation: round_metric(
                    avg_metric(&group, |sample| sample.time_normalization_deviation),
                    4,
                ),
                last_seen_at,
                status: status.to_string(),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then(left.pattern_key.cmp(&right.pattern_key))
    });
    rows
}

fn reasoning_token_rows(samples: &[CodexReasoningAnalyticsSample]) -> Vec<CodexReasoningTokenRow> {
    let mut groups: HashMap<i64, Vec<CodexReasoningAnalyticsSample>> = HashMap::new();
    for sample in samples {
        if let Some(tokens) = sample.reasoning_tokens {
            groups.entry(tokens).or_default().push(sample.clone());
        }
    }
    let mut rows = groups
        .into_iter()
        .map(|(value, group)| {
            let count = group.len() as f64;
            CodexReasoningTokenRow {
                value,
                count: count as i64,
                final_answer_only_ratio: round_metric(
                    group
                        .iter()
                        .filter(|sample| sample.final_answer_only)
                        .count() as f64
                        / count,
                    6,
                ),
                commentary_observed_ratio: round_metric(
                    group
                        .iter()
                        .filter(|sample| sample.commentary_observed)
                        .count() as f64
                        / count,
                    6,
                ),
                avg_duration_total_ms: round_metric(
                    avg_metric(&group, |sample| {
                        sample.duration_total_ms.map(|value| value as f64)
                    }),
                    2,
                ),
                avg_output_tps: round_metric(avg_metric(&group, |sample| sample.output_tps), 4),
                avg_time_normalization_deviation: round_metric(
                    avg_metric(&group, |sample| sample.time_normalization_deviation),
                    4,
                ),
                last_seen_at: group.iter().map(|sample| sample.ts.clone()).max(),
            }
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then(left.value.cmp(&right.value))
    });
    rows
}

fn snapshot_from_samples(
    mut samples: Vec<CodexReasoningAnalyticsSample>,
    date_from: Option<String>,
    date_to: Option<String>,
    recent_limit: u32,
) -> CodexReasoningAnalyticsSnapshot {
    samples.sort_by(|left, right| right.ts.cmp(&left.ts));
    let total = samples.len() as i64;
    let final_only = samples
        .iter()
        .filter(|sample| sample.final_answer_only)
        .count() as f64;
    let commentary = samples
        .iter()
        .filter(|sample| sample.commentary_observed)
        .count() as f64;
    let continuation_recovery_count: i64 = samples
        .iter()
        .map(|sample| sample.continuation_recovery_count)
        .sum();
    let continuation_recovery_success_count: i64 = samples
        .iter()
        .map(|sample| sample.continuation_recovery_success_count)
        .sum();
    let by_model_family = group_samples(&samples, |sample| {
        sample.effective_local_model_family.clone()
    })
    .into_iter()
    .map(|row| CodexReasoningModelFamilyRow {
        model_family: row.key.clone(),
        row,
    })
    .collect();
    let by_reasoning_effort = group_samples(&samples, |sample| {
        sample
            .request_reasoning_effort
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    })
    .into_iter()
    .map(|row| CodexReasoningEffortRow {
        reasoning_effort: row.key.clone(),
        row,
    })
    .collect();
    let by_model_family_and_effort = group_samples(&samples, |sample| {
        format!(
            "{}|{}",
            sample.effective_local_model_family,
            sample
                .request_reasoning_effort
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        )
    })
    .into_iter()
    .map(|row| {
        let (model_family, reasoning_effort) = row
            .key
            .split_once('|')
            .unwrap_or((row.key.as_str(), "unknown"));
        CodexReasoningFamilyEffortRow {
            group_key: row.key.clone(),
            group_label: format!("{model_family} / {reasoning_effort}"),
            model_family: model_family.to_string(),
            reasoning_effort: reasoning_effort.to_string(),
            row,
        }
    })
    .collect();

    CodexReasoningAnalyticsSnapshot {
        ok: true,
        schema_version: SCHEMA_VERSION,
        analytics_ready: true,
        date_from,
        date_to,
        summary: CodexReasoningAnalyticsSummary {
            total_samples: total,
            continuation_recovery_count,
            continuation_recovery_success_count,
            continuation_recovery_success_ratio: if continuation_recovery_count == 0 {
                0.0
            } else {
                round_metric(
                    continuation_recovery_success_count as f64
                        / continuation_recovery_count as f64,
                    6,
                )
            },
            final_answer_only_ratio: if total == 0 {
                0.0
            } else {
                round_metric(final_only / total as f64, 6)
            },
            commentary_present_ratio: if total == 0 {
                0.0
            } else {
                round_metric(commentary / total as f64, 6)
            },
            commentary_observed_ratio: if total == 0 {
                0.0
            } else {
                round_metric(commentary / total as f64, 6)
            },
            avg_duration_total_ms: round_metric(
                avg_metric(&samples, |sample| sample.duration_total_ms.map(|value| value as f64)),
                2,
            ),
            avg_output_tps: round_metric(avg_metric(&samples, |sample| sample.output_tps), 4),
            avg_reasoning_adjusted_tps: round_metric(
                avg_metric(&samples, |sample| sample.reasoning_adjusted_tps),
                4,
            ),
            wording: "统计结果只表示可观测结构信号，用于发现候选异常特征，不代表最终归因，也不证明模型内部没有思考。".to_string(),
        },
        top_reasoning_tokens: top_reasoning_tokens(&samples, 8),
        output_tps_buckets: output_tps_buckets(&samples),
        by_model_family,
        by_reasoning_effort,
        by_model_family_and_effort,
        by_reasoning_token: reasoning_token_rows(&samples),
        candidate_patterns: candidate_patterns(&samples),
        recent_samples: samples.into_iter().take(recent_limit as usize).collect(),
    }
}

pub fn snapshot(
    db: &db::Db,
    input: CodexReasoningAnalyticsSnapshotInput,
) -> AppResult<CodexReasoningAnalyticsSnapshot> {
    backfill_from_request_logs(
        db,
        CodexReasoningAnalyticsBackfillInput {
            since_created_at_ms: None,
            limit: Some(DEFAULT_BACKFILL_LIMIT),
        },
    )?;
    let date_from = validate_date_key(input.date_from.as_deref())?;
    let date_to = validate_date_key(input.date_to.as_deref())?;
    let recent_limit = normalize_limit(input.recent_limit, DEFAULT_RECENT_LIMIT, MAX_RECENT_LIMIT);
    let samples = load_samples(
        db,
        date_from.as_deref(),
        date_to.as_deref(),
        input.since_created_at_ms,
        None,
    )?;
    Ok(snapshot_from_samples(
        samples,
        date_from,
        date_to,
        recent_limit,
    ))
}

fn csv_escape(value: impl ToString) -> String {
    format!("\"{}\"", value.to_string().replace('"', "\"\""))
}

fn export_csv(samples: &[CodexReasoningAnalyticsSample]) -> String {
    let headers = [
        "sample_id",
        "gateway_request_id",
        "request_log_id",
        "ts",
        "path",
        "method",
        "request_kind",
        "intercept_exempt_reason",
        "request_model",
        "effective_local_model_family",
        "request_reasoning_effort",
        "reasoning_tokens",
        "output_tokens",
        "total_tokens",
        "duration_total_ms",
        "output_tps",
        "reasoning_adjusted_tps",
        "final_answer_only",
        "has_commentary",
        "commentary_observed",
        "has_final_answer",
        "has_tool_call",
        "has_reasoning_item",
        "matched_current_rule",
        "blocked_by_gateway",
        "internal_retry_attempt_index",
        "internal_retry_remaining",
        "final_action",
        "upstream_http_status",
        "client_http_status",
        "source_kind",
        "source_name",
    ];
    let mut lines = vec![headers.join(",")];
    for sample in samples {
        lines.push(
            [
                csv_escape(&sample.sample_id),
                csv_escape(&sample.gateway_request_id),
                csv_escape(
                    sample
                        .request_log_id
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(&sample.ts),
                csv_escape(&sample.path),
                csv_escape(&sample.method),
                csv_escape(&sample.request_kind),
                csv_escape(sample.intercept_exempt_reason.clone().unwrap_or_default()),
                csv_escape(sample.request_model.clone().unwrap_or_default()),
                csv_escape(&sample.effective_local_model_family),
                csv_escape(sample.request_reasoning_effort.clone().unwrap_or_default()),
                csv_escape(
                    sample
                        .reasoning_tokens
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(
                    sample
                        .output_tokens
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(
                    sample
                        .total_tokens
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(
                    sample
                        .duration_total_ms
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(sample.output_tps.map(|v| v.to_string()).unwrap_or_default()),
                csv_escape(
                    sample
                        .reasoning_adjusted_tps
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(sample.final_answer_only),
                csv_escape(sample.has_commentary),
                csv_escape(sample.commentary_observed),
                csv_escape(sample.has_final_answer),
                csv_escape(sample.has_tool_call),
                csv_escape(sample.has_reasoning_item),
                csv_escape(sample.matched_current_rule),
                csv_escape(sample.blocked_by_gateway),
                csv_escape(
                    sample
                        .internal_retry_attempt_index
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(
                    sample
                        .internal_retry_remaining
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(&sample.final_action),
                csv_escape(
                    sample
                        .upstream_http_status
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(
                    sample
                        .client_http_status
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ),
                csv_escape(&sample.source_kind),
                csv_escape(sample.source_name.clone().unwrap_or_default()),
            ]
            .join(","),
        );
    }
    lines.join("\n")
}

pub fn export(
    db: &db::Db,
    input: CodexReasoningAnalyticsExportInput,
) -> AppResult<CodexReasoningAnalyticsExport> {
    let date_from = validate_date_key(input.date_from.as_deref())?;
    let date_to = validate_date_key(input.date_to.as_deref())?;
    let samples = load_samples(
        db,
        date_from.as_deref(),
        date_to.as_deref(),
        input.since_created_at_ms,
        None,
    )?;
    let suffix = match (&date_from, &date_to, input.since_created_at_ms) {
        (Some(from), Some(to), _) => format!("{from}_{to}"),
        (Some(from), None, _) => format!("{from}_latest"),
        (None, Some(to), _) => format!("until_{to}"),
        (None, None, Some(_)) => "session".to_string(),
        (None, None, None) => "all".to_string(),
    };
    match input.format {
        CodexReasoningAnalyticsExportFormat::Json => {
            let snapshot = snapshot_from_samples(samples, date_from, date_to, DEFAULT_RECENT_LIMIT);
            Ok(CodexReasoningAnalyticsExport {
                format: "json".to_string(),
                file_name: format!("aio-reasoning-analytics-{suffix}.json"),
                content: serde_json::to_string_pretty(&snapshot)
                    .map_err(|e| db_err!("failed to encode reasoning analytics export: {e}"))?,
            })
        }
        CodexReasoningAnalyticsExportFormat::Csv => Ok(CodexReasoningAnalyticsExport {
            format: "csv".to_string(),
            file_name: format!("aio-reasoning-analytics-{suffix}.csv"),
            content: export_csv(&samples),
        }),
    }
}

fn field_coverage(samples: &[CodexReasoningAnalyticsSample]) -> CodexReasoningFieldCoverage {
    let total = samples.len().max(1) as f64;
    let ratio = |count: usize| round_metric(count as f64 / total, 6);
    CodexReasoningFieldCoverage {
        reasoning_tokens: ratio(
            samples
                .iter()
                .filter(|sample| sample.reasoning_tokens.is_some())
                .count(),
        ),
        final_answer_only: ratio(samples.len()),
        commentary_observed: ratio(samples.len()),
        duration_total_ms: ratio(
            samples
                .iter()
                .filter(|sample| sample.duration_total_ms.is_some())
                .count(),
        ),
        output_tokens: ratio(
            samples
                .iter()
                .filter(|sample| sample.output_tokens.is_some() || sample.total_tokens.is_some())
                .count(),
        ),
        model_family: ratio(
            samples
                .iter()
                .filter(|sample| sample.effective_local_model_family != "unknown")
                .count(),
        ),
        reasoning_effort: ratio(
            samples
                .iter()
                .filter(|sample| sample.request_reasoning_effort.is_some())
                .count(),
        ),
        status: ratio(
            samples
                .iter()
                .filter(|sample| {
                    sample.client_http_status.is_some()
                        || sample.upstream_http_status.is_some()
                        || !sample.final_action.is_empty()
                })
                .count(),
        ),
        retry_status: ratio(
            samples
                .iter()
                .filter(|sample| {
                    sample.internal_retry_attempt_index.is_some()
                        || sample.internal_retry_remaining.is_some()
                })
                .count(),
        ),
        blocked_status: ratio(samples.len()),
    }
}

pub fn analyze(
    db: &db::Db,
    input: CodexReasoningAnalyticsAnalyzeInput,
) -> AppResult<CodexReasoningAnalysisResult> {
    let date_from = validate_date_key(input.date_from.as_deref())?;
    let date_to = validate_date_key(input.date_to.as_deref())?;
    let samples = load_samples(
        db,
        date_from.as_deref(),
        date_to.as_deref(),
        input.since_created_at_ms,
        None,
    )?;
    let coverage = field_coverage(&samples);
    let mut missing_core = Vec::new();
    if coverage.reasoning_tokens <= 0.0 {
        missing_core.push("reasoning_tokens".to_string());
    }
    if coverage.final_answer_only <= 0.0 {
        missing_core.push("final_answer_only".to_string());
    }
    if coverage.commentary_observed <= 0.0 {
        missing_core.push("commentary_observed".to_string());
    }
    let analysis_value = if samples.is_empty() || !missing_core.is_empty() {
        "no_analysis_value"
    } else if coverage.duration_total_ms <= 0.0
        || coverage.output_tokens <= 0.0
        || coverage.model_family <= 0.0
        || coverage.reasoning_effort <= 0.0
    {
        "partial"
    } else {
        "valuable"
    };
    let tokens = input.reasoning_tokens.unwrap_or_else(|| vec![516]);
    let candidate_samples = if analysis_value == "valuable" {
        samples
            .iter()
            .filter(|sample| {
                sample
                    .reasoning_tokens
                    .is_some_and(|value| tokens.contains(&value))
                    && sample.final_answer_only
                    && !sample.commentary_observed
                    && sample.time_normalization_deviation.unwrap_or(0.0) >= 0.5
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let candidate_ids = candidate_samples
        .iter()
        .map(|sample| sample.sample_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let baseline_samples = samples
        .iter()
        .filter(|sample| !candidate_ids.contains(&sample.sample_id))
        .cloned()
        .collect::<Vec<_>>();
    let candidate_count = candidate_samples.len() as i64;
    let baseline_count = baseline_samples.len() as i64;
    let conclusion = if analysis_value == "no_analysis_value" {
        "no_analysis_value"
    } else if analysis_value == "partial" {
        "insufficient_fields"
    } else if candidate_count >= 3 {
        "strong_candidate"
    } else if candidate_count > 0 {
        "candidate"
    } else {
        "not_observed"
    };
    let candidate_not_observed = candidate_samples
        .iter()
        .filter(|sample| !sample.commentary_observed)
        .count() as i64;
    Ok(CodexReasoningAnalysisResult {
        ok: true,
        analysis_profile: ANALYSIS_PROFILE_NAME.to_string(),
        analysis_value: analysis_value.to_string(),
        conclusion: conclusion.to_string(),
        field_coverage: coverage,
        missing_core_fields: missing_core.clone(),
        decision_reason: if samples.is_empty() {
            "没有可用于 reasoning 行为分析的结构化样本。".to_string()
        } else if !missing_core.is_empty() {
            format!("缺少核心字段：{}。", missing_core.join(", "))
        } else if analysis_value == "partial" {
            "辅助字段不足；只能展示覆盖率，不能给强候选结论。".to_string()
        } else {
            "核心字段覆盖率足够，可以进入特征分析。".to_string()
        },
        sample_count: samples.len() as i64,
        candidate_summary: CodexReasoningCandidateSummary {
            candidate_count,
            candidate_ratio: if samples.is_empty() {
                0.0
            } else {
                round_metric(candidate_count as f64 / samples.len() as f64, 6)
            },
            reasoning_516_count: candidate_samples
                .iter()
                .filter(|sample| sample.reasoning_tokens == Some(516))
                .count() as i64,
            final_answer_only_count: candidate_samples
                .iter()
                .filter(|sample| sample.final_answer_only)
                .count() as i64,
            commentary_not_observed_count: candidate_not_observed,
            high_time_normalization_deviation_count: candidate_samples
                .iter()
                .filter(|sample| sample.time_normalization_deviation.unwrap_or(0.0) >= 0.5)
                .count() as i64,
            last_seen_at: candidate_samples
                .iter()
                .map(|sample| sample.ts.clone())
                .max(),
        },
        baseline_comparison: CodexReasoningBaselineComparison {
            baseline_count,
            candidate_avg_time_normalization_deviation: round_metric(
                avg_metric(&candidate_samples, |sample| {
                    sample.time_normalization_deviation
                }),
                6,
            ),
            baseline_avg_time_normalization_deviation: round_metric(
                avg_metric(&baseline_samples, |sample| {
                    sample.time_normalization_deviation
                }),
                6,
            ),
            candidate_final_answer_only_ratio: if candidate_count == 0 {
                0.0
            } else {
                round_metric(
                    candidate_samples
                        .iter()
                        .filter(|sample| sample.final_answer_only)
                        .count() as f64
                        / candidate_count as f64,
                    6,
                )
            },
            baseline_final_answer_only_ratio: if baseline_count == 0 {
                0.0
            } else {
                round_metric(
                    baseline_samples
                        .iter()
                        .filter(|sample| sample.final_answer_only)
                        .count() as f64
                        / baseline_count as f64,
                    6,
                )
            },
            candidate_commentary_not_observed_ratio: if candidate_count == 0 {
                0.0
            } else {
                round_metric(candidate_not_observed as f64 / candidate_count as f64, 6)
            },
            baseline_commentary_not_observed_ratio: if baseline_count == 0 {
                0.0
            } else {
                round_metric(
                    baseline_samples
                        .iter()
                        .filter(|sample| !sample.commentary_observed)
                        .count() as f64
                        / baseline_count as f64,
                    6,
                )
            },
        },
        samples_preview: if candidate_samples.is_empty() {
            samples.into_iter().take(20).collect()
        } else {
            candidate_samples.into_iter().take(20).collect()
        },
    })
}
