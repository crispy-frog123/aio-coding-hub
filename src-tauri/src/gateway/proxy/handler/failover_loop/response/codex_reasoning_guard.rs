//! Usage: Codex degraded-reasoning detection helpers.

use crate::gateway::events::{decision_chain as dc, FailoverAttempt};
use crate::gateway::proxy::ErrorCategory;
use crate::gateway::response_fixer;
use crate::settings::CodexReasoningGuardCompareMode;
use axum::http::StatusCode;
use std::sync::{Arc, Mutex};

pub(super) const CODEX_REASONING_GUARD_ERROR_CODE: &str = "GW_CODEX_REASONING_GUARD";
pub(super) const CODEX_REASONING_GUARD_REASON_CODE: &str = "codex_reasoning_guard";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexReasoningGuardMatch {
    pub(super) reasoning_tokens: i64,
    pub(super) pointer: &'static str,
    pub(super) compare_mode: CodexReasoningGuardCompareMode,
    pub(super) matched_rule_value: i64,
}

const REASONING_POINTERS: &[&str] = &[
    "/usage/output_tokens_details/reasoning_tokens",
    "/usage/completion_tokens_details/reasoning_tokens",
    "/response/usage/output_tokens_details/reasoning_tokens",
    "/response/usage/completion_tokens_details/reasoning_tokens",
];

pub(super) fn detect_from_json(
    cli_key: &str,
    value: &serde_json::Value,
    compare_mode: CodexReasoningGuardCompareMode,
    configured_values: &[i64],
) -> Option<CodexReasoningGuardMatch> {
    if cli_key != "codex" || configured_values.is_empty() {
        return None;
    }

    for pointer in REASONING_POINTERS {
        let Some(raw) = value.pointer(pointer) else {
            continue;
        };
        let reasoning_tokens = match raw {
            serde_json::Value::Number(number) => number
                .as_i64()
                .or_else(|| number.as_u64().and_then(|v| i64::try_from(v).ok())),
            _ => None,
        };
        let Some(reasoning_tokens) = reasoning_tokens else {
            continue;
        };
        if let Some(matched_rule_value) =
            find_matched_rule_value(compare_mode, reasoning_tokens, configured_values)
        {
            return Some(CodexReasoningGuardMatch {
                reasoning_tokens,
                pointer,
                compare_mode,
                matched_rule_value,
            });
        }
    }

    None
}

fn find_matched_rule_value(
    compare_mode: CodexReasoningGuardCompareMode,
    reasoning_tokens: i64,
    configured_values: &[i64],
) -> Option<i64> {
    match compare_mode {
        CodexReasoningGuardCompareMode::Equals => configured_values
            .iter()
            .copied()
            .find(|value| *value == reasoning_tokens),
        CodexReasoningGuardCompareMode::LessThanOrEqual => configured_values
            .iter()
            .copied()
            .filter(|value| reasoning_tokens <= *value)
            .min(),
    }
}

fn compare_mode_symbol(compare_mode: CodexReasoningGuardCompareMode) -> &'static str {
    match compare_mode {
        CodexReasoningGuardCompareMode::Equals => "==",
        CodexReasoningGuardCompareMode::LessThanOrEqual => "<=",
    }
}

pub(super) fn push_special_setting(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    provider_id: i64,
    provider_name: &str,
    retry_index: u32,
    matched: &CodexReasoningGuardMatch,
) {
    response_fixer::push_special_setting(
        special_settings,
        serde_json::json!({
            "type": "codex_reasoning_guard",
            "scope": "attempt",
            "hit": true,
            "providerId": provider_id,
            "providerName": provider_name,
            "reasoningTokens": matched.reasoning_tokens,
            "compareMode": matched.compare_mode,
            "compareModeSymbol": compare_mode_symbol(matched.compare_mode),
            "matchedRuleValue": matched.matched_rule_value,
            "pointer": matched.pointer,
            "retryAttemptNumber": retry_index,
            "retryAttemptNumberNext": retry_index.saturating_add(1),
            "displayStatus": StatusCode::BAD_GATEWAY.as_u16(),
            "action": "retry_same_provider_no_circuit",
        }),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_guard_retry_attempt(
    attempts: &mut Vec<FailoverAttempt>,
    provider_id: i64,
    provider_name: &str,
    base_url: &str,
    provider_index: u32,
    retry_index: u32,
    session_reuse: Option<bool>,
    attempt_started_ms: u128,
    attempt_duration_ms: u128,
    circuit_state_before: &'static str,
    circuit_failure_count: u32,
    circuit_failure_threshold: u32,
    matched: &CodexReasoningGuardMatch,
) {
    attempts.push(FailoverAttempt {
        provider_id,
        provider_name: provider_name.to_string(),
        base_url: base_url.to_string(),
        outcome: "codex_reasoning_guard_retry".to_string(),
        status: Some(StatusCode::BAD_GATEWAY.as_u16()),
        provider_index: Some(provider_index),
        retry_index: Some(retry_index),
        session_reuse,
        error_category: Some(ErrorCategory::SystemError.as_str()),
        error_code: Some(CODEX_REASONING_GUARD_ERROR_CODE),
        decision: Some("retry_same_provider"),
        reason: Some(format!(
            "codex reasoning guard matched reasoning_tokens={} {} {} via {}",
            matched.reasoning_tokens,
            compare_mode_symbol(matched.compare_mode),
            matched.matched_rule_value,
            matched.pointer
        )),
        selection_method: dc::selection_method(provider_index, retry_index, session_reuse),
        reason_code: Some(CODEX_REASONING_GUARD_REASON_CODE),
        attempt_started_ms: Some(attempt_started_ms),
        attempt_duration_ms: Some(attempt_duration_ms),
        circuit_state_before: Some(circuit_state_before),
        circuit_state_after: Some(circuit_state_before),
        circuit_failure_count: Some(circuit_failure_count),
        circuit_failure_threshold: Some(circuit_failure_threshold),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_from_json_matches_equals_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 516 } }
        });

        let matched = detect_from_json(
            "codex",
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516, 1024],
        )
        .expect("should match");

        assert_eq!(matched.reasoning_tokens, 516);
        assert_eq!(matched.matched_rule_value, 516);
        assert_eq!(matched.compare_mode, CodexReasoningGuardCompareMode::Equals);
    }

    #[test]
    fn detect_from_json_does_not_match_equals_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 300 } }
        });

        let matched = detect_from_json(
            "codex",
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516],
        );

        assert!(matched.is_none());
    }

    #[test]
    fn detect_from_json_matches_less_than_or_equal_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 300 } }
        });

        let matched = detect_from_json(
            "codex",
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[516],
        )
        .expect("should match");

        assert_eq!(matched.reasoning_tokens, 300);
        assert_eq!(matched.matched_rule_value, 516);
        assert_eq!(
            matched.compare_mode,
            CodexReasoningGuardCompareMode::LessThanOrEqual
        );
    }

    #[test]
    fn detect_from_json_uses_smallest_matching_less_than_or_equal_threshold() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 300 } }
        });

        let matched = detect_from_json(
            "codex",
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[1024, 516, 2048],
        )
        .expect("should match");

        assert_eq!(matched.matched_rule_value, 516);
    }

    #[test]
    fn detect_from_json_does_not_match_less_than_or_equal_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 800 } }
        });

        let matched = detect_from_json(
            "codex",
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[516],
        );

        assert!(matched.is_none());
    }
}
