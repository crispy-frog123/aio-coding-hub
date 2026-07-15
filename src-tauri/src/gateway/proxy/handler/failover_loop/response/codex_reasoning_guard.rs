//! Usage: Codex degraded-reasoning detection helpers.

use crate::gateway::events::{decision_chain as dc, FailoverAttempt};
use crate::gateway::proxy::request_context::CodexRequestKind;
use crate::gateway::proxy::ErrorCategory;
use crate::gateway::response_fixer;
use crate::settings::{
    CodexReasoningGuardCompareMode, CodexReasoningGuardExhaustedAction,
    CodexReasoningGuardMatchMode, CodexReasoningGuardModelRule, CodexReasoningGuardRuleMode,
    CodexReasoningGuardStreamAction,
};
use axum::http::StatusCode;
use std::sync::{Arc, Mutex};

pub(super) const CODEX_REASONING_GUARD_ERROR_CODE: &str = "GW_CODEX_REASONING_GUARD";
pub(super) const CODEX_REASONING_GUARD_REASON_CODE: &str = "codex_reasoning_guard";
const CODEX_REASONING_GUARD_RULE_SOURCE_GLOBAL_DEFAULT: &str = "global_default";
const CODEX_REASONING_GUARD_RULE_SOURCE_MODEL_RULE: &str = "model_rule";
const CODEX_REASONING_GUARD_RULE_SOURCE_FINAL_ANSWER_ONLY: &str = "final_answer_only";
const CODEX_REASONING_GUARD_RULE_SOURCE_FORMULA_518N_MINUS_2: &str = "formula_518n_minus_2";
const CONTINUATION_ENCRYPTED_INCLUDE: &str = "reasoning.encrypted_content";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexReasoningGuardMatch {
    pub(super) rule_mode: CodexReasoningGuardRuleMode,
    pub(super) match_mode: CodexReasoningGuardMatchMode,
    pub(super) reasoning_tokens: Option<i64>,
    pub(super) pointer: Option<&'static str>,
    pub(super) compare_mode: CodexReasoningGuardCompareMode,
    pub(super) matched_rule_value: Option<i64>,
    pub(super) requested_model: Option<String>,
    pub(super) rule_source: &'static str,
    pub(super) rule_model: Option<String>,
    pub(super) final_answer_only: bool,
    pub(super) commentary_observed: bool,
    pub(super) has_tool_call: bool,
    pub(super) has_reasoning_item: bool,
    pub(super) request_kind: CodexRequestKind,
    pub(super) intercept_exempt_reason: Option<&'static str>,
    pub(super) reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct CodexResponseStructure {
    pub(super) has_final_answer: bool,
    pub(super) has_output_text: bool,
    pub(super) has_commentary: bool,
    pub(super) has_tool_call: bool,
    pub(super) has_reasoning_item: bool,
}

impl CodexResponseStructure {
    pub(super) fn final_answer_only(&self) -> bool {
        self.has_final_answer
            && !self.has_commentary
            && !self.has_tool_call
            && !self.has_reasoning_item
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexReasoningObservation {
    pub(super) reasoning_tokens: Option<i64>,
    pub(super) reasoning_pointer: Option<&'static str>,
    pub(super) structure: CodexResponseStructure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodexReasoningGuardEvaluation {
    pub(super) checked: bool,
    pub(super) rule_mode: CodexReasoningGuardRuleMode,
    pub(super) match_mode: CodexReasoningGuardMatchMode,
    pub(super) observation: CodexReasoningObservation,
    pub(super) matched: Option<CodexReasoningGuardMatch>,
    pub(super) miss_reason: Option<&'static str>,
    pub(super) intercept_exempt_reason: Option<&'static str>,
    pub(super) requested_model: Option<String>,
    pub(super) reasoning_effort: Option<String>,
    pub(super) request_kind: CodexRequestKind,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedCodexReasoningGuardRule<'a> {
    compare_mode: CodexReasoningGuardCompareMode,
    configured_values: &'a [i64],
    rule_source: &'static str,
    rule_model: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct CodexReasoningGuardDetectionOptions<'a> {
    pub(super) rule_mode: CodexReasoningGuardRuleMode,
    pub(super) match_mode: CodexReasoningGuardMatchMode,
    pub(super) request_kind: CodexRequestKind,
    pub(super) requested_reasoning_effort: Option<&'a str>,
    pub(super) fallback_compare_mode: CodexReasoningGuardCompareMode,
    pub(super) fallback_values: &'a [i64],
    pub(super) model_rules: &'a [CodexReasoningGuardModelRule],
}

const REASONING_POINTERS: &[&str] = &[
    "/usage/output_tokens_details/reasoning_tokens",
    "/usage/completion_tokens_details/reasoning_tokens",
    "/response/usage/output_tokens_details/reasoning_tokens",
    "/response/usage/completion_tokens_details/reasoning_tokens",
];

#[cfg(test)]
fn detect_from_json(
    cli_key: &str,
    requested_model: Option<&str>,
    value: &serde_json::Value,
    options: CodexReasoningGuardDetectionOptions<'_>,
) -> Option<CodexReasoningGuardMatch> {
    evaluate_from_json(cli_key, requested_model, value, options).matched
}

pub(super) fn evaluate_from_json(
    cli_key: &str,
    requested_model: Option<&str>,
    value: &serde_json::Value,
    options: CodexReasoningGuardDetectionOptions<'_>,
) -> CodexReasoningGuardEvaluation {
    let observation = observe_response(value);
    let requested_model = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned);
    let reasoning_effort = options
        .requested_reasoning_effort
        .map(str::trim)
        .filter(|effort| !effort.is_empty())
        .map(|effort| effort.to_ascii_lowercase());

    if cli_key != "codex" {
        return CodexReasoningGuardEvaluation {
            checked: false,
            rule_mode: options.rule_mode,
            match_mode: options.match_mode,
            observation,
            matched: None,
            miss_reason: Some("not_codex_request"),
            intercept_exempt_reason: None,
            requested_model,
            reasoning_effort,
            request_kind: options.request_kind,
        };
    }

    if is_context_compaction_exempt(options.request_kind, observation.reasoning_tokens) {
        return CodexReasoningGuardEvaluation {
            checked: true,
            rule_mode: options.rule_mode,
            match_mode: options.match_mode,
            observation,
            matched: None,
            miss_reason: Some("context_compaction_exempt"),
            intercept_exempt_reason: Some("context_compaction"),
            requested_model,
            reasoning_effort,
            request_kind: options.request_kind,
        };
    }

    let (matched, miss_reason) = match options.rule_mode {
        CodexReasoningGuardRuleMode::ReasoningTokens => {
            if options.match_mode == CodexReasoningGuardMatchMode::Formula518nMinus2 {
                let Some(reasoning_tokens) = observation.reasoning_tokens else {
                    return CodexReasoningGuardEvaluation {
                        checked: true,
                        rule_mode: options.rule_mode,
                        match_mode: options.match_mode,
                        observation,
                        matched: None,
                        miss_reason: Some("missing_reasoning_tokens"),
                        intercept_exempt_reason: None,
                        requested_model,
                        reasoning_effort,
                        request_kind: options.request_kind,
                    };
                };
                if !matches_formula_518n_minus_2(reasoning_tokens) {
                    (None, Some("reasoning_tokens_not_formula_match"))
                } else {
                    (
                        Some(CodexReasoningGuardMatch {
                            rule_mode: options.rule_mode,
                            match_mode: options.match_mode,
                            reasoning_tokens: Some(reasoning_tokens),
                            pointer: observation.reasoning_pointer,
                            compare_mode: CodexReasoningGuardCompareMode::Equals,
                            matched_rule_value: Some(reasoning_tokens),
                            requested_model: requested_model.clone(),
                            rule_source: CODEX_REASONING_GUARD_RULE_SOURCE_FORMULA_518N_MINUS_2,
                            rule_model: None,
                            final_answer_only: observation.structure.final_answer_only(),
                            commentary_observed: observation.structure.has_commentary,
                            has_tool_call: observation.structure.has_tool_call,
                            has_reasoning_item: observation.structure.has_reasoning_item,
                            request_kind: options.request_kind,
                            intercept_exempt_reason: None,
                            reasoning_effort: reasoning_effort.clone(),
                        }),
                        None,
                    )
                }
            } else {
                let Some(resolved_rule) = resolve_guard_rule(
                    requested_model.as_deref(),
                    options.fallback_compare_mode,
                    options.fallback_values,
                    options.model_rules,
                ) else {
                    return CodexReasoningGuardEvaluation {
                        checked: true,
                        rule_mode: options.rule_mode,
                        match_mode: options.match_mode,
                        observation,
                        matched: None,
                        miss_reason: Some("no_configured_reasoning_rule"),
                        intercept_exempt_reason: None,
                        requested_model,
                        reasoning_effort,
                        request_kind: options.request_kind,
                    };
                };
                let Some(reasoning_tokens) = observation.reasoning_tokens else {
                    return CodexReasoningGuardEvaluation {
                        checked: true,
                        rule_mode: options.rule_mode,
                        match_mode: options.match_mode,
                        observation,
                        matched: None,
                        miss_reason: Some("missing_reasoning_tokens"),
                        intercept_exempt_reason: None,
                        requested_model,
                        reasoning_effort,
                        request_kind: options.request_kind,
                    };
                };
                if let Some(matched_rule_value) = find_matched_rule_value(
                    resolved_rule.compare_mode,
                    reasoning_tokens,
                    resolved_rule.configured_values,
                ) {
                    (
                        Some(CodexReasoningGuardMatch {
                            rule_mode: options.rule_mode,
                            match_mode: options.match_mode,
                            reasoning_tokens: Some(reasoning_tokens),
                            pointer: observation.reasoning_pointer,
                            compare_mode: resolved_rule.compare_mode,
                            matched_rule_value: Some(matched_rule_value),
                            requested_model: requested_model.clone(),
                            rule_source: resolved_rule.rule_source,
                            rule_model: resolved_rule.rule_model.map(ToOwned::to_owned),
                            final_answer_only: observation.structure.final_answer_only(),
                            commentary_observed: observation.structure.has_commentary,
                            has_tool_call: observation.structure.has_tool_call,
                            has_reasoning_item: observation.structure.has_reasoning_item,
                            request_kind: options.request_kind,
                            intercept_exempt_reason: None,
                            reasoning_effort: reasoning_effort.clone(),
                        }),
                        None,
                    )
                } else {
                    (None, Some("reasoning_tokens_not_matched"))
                }
            }
        }
        CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh => {
            if !should_intercept_final_answer_only_reasoning(observation.reasoning_tokens) {
                (None, Some("zero_reasoning_tokens"))
            } else if !is_final_answer_only_intercept_effort(reasoning_effort.as_deref()) {
                (None, Some("reasoning_effort_not_high_xhigh"))
            } else if !observation.structure.has_final_answer {
                (None, Some("missing_final_answer"))
            } else if observation.structure.has_commentary {
                (None, Some("commentary_observed"))
            } else if observation.structure.has_tool_call {
                (None, Some("tool_call_observed"))
            } else if observation.structure.has_reasoning_item {
                (None, Some("reasoning_item_observed"))
            } else {
                (
                    Some(CodexReasoningGuardMatch {
                        rule_mode: options.rule_mode,
                        match_mode: options.match_mode,
                        reasoning_tokens: observation.reasoning_tokens,
                        pointer: observation.reasoning_pointer,
                        compare_mode: options.fallback_compare_mode,
                        matched_rule_value: None,
                        requested_model: requested_model.clone(),
                        rule_source: CODEX_REASONING_GUARD_RULE_SOURCE_FINAL_ANSWER_ONLY,
                        rule_model: None,
                        final_answer_only: true,
                        commentary_observed: false,
                        has_tool_call: false,
                        has_reasoning_item: false,
                        request_kind: options.request_kind,
                        intercept_exempt_reason: None,
                        reasoning_effort: reasoning_effort.clone(),
                    }),
                    None,
                )
            }
        }
    };

    CodexReasoningGuardEvaluation {
        checked: true,
        rule_mode: options.rule_mode,
        match_mode: options.match_mode,
        observation,
        matched,
        miss_reason,
        intercept_exempt_reason: None,
        requested_model,
        reasoning_effort,
        request_kind: options.request_kind,
    }
}

pub(super) fn observe_response(value: &serde_json::Value) -> CodexReasoningObservation {
    let (reasoning_tokens, reasoning_pointer) = extract_reasoning_tokens(value);
    let mut structure = CodexResponseStructure::default();
    observe_structure_value(value, false, &mut structure);
    CodexReasoningObservation {
        reasoning_tokens,
        reasoning_pointer,
        structure,
    }
}

fn extract_reasoning_tokens(value: &serde_json::Value) -> (Option<i64>, Option<&'static str>) {
    for pointer in REASONING_POINTERS {
        let Some(raw) = value.pointer(pointer) else {
            continue;
        };
        if let Some(reasoning_tokens) = parse_reasoning_tokens(raw) {
            return (Some(reasoning_tokens), Some(pointer));
        }
    }
    (None, None)
}

fn parse_reasoning_tokens(raw: &serde_json::Value) -> Option<i64> {
    match raw {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|v| i64::try_from(v).ok())),
        serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn observe_structure_value(
    value: &serde_json::Value,
    in_visible_output: bool,
    structure: &mut CodexResponseStructure,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                observe_structure_value(item, in_visible_output, structure);
            }
        }
        serde_json::Value::Object(object) => {
            let type_name = object
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let channel = object
                .get("channel")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if channel == "commentary" {
                structure.has_commentary = true;
            }
            if is_tool_call_type(&type_name)
                || object
                    .get("tool_calls")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|items| !items.is_empty())
                || object
                    .get("function_call")
                    .is_some_and(|value| !value.is_null())
            {
                structure.has_tool_call = true;
            }
            if is_reasoning_item_type(&type_name) {
                structure.has_reasoning_item = true;
            }

            let role = object
                .get("role")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let is_visible_text_item = type_name == "output_text" || type_name == "text";
            let is_assistant_message = type_name == "message" && role == "assistant";
            let visible_here = in_visible_output || is_visible_text_item || is_assistant_message;

            if is_visible_text_item {
                mark_text_if_present(object.get("text"), structure);
            }
            mark_text_if_present(object.get("output_text"), structure);
            if visible_here && !is_reasoning_item_type(&type_name) {
                mark_text_if_present(object.get("text"), structure);
            }

            for (key, child) in object {
                let child_visible = match key.as_str() {
                    "content" => visible_here || is_assistant_message,
                    "output" => false,
                    _ => visible_here && (key == "text" || key == "output_text"),
                };
                if key == "reasoning" && !child.is_array() {
                    continue;
                }
                observe_structure_value(child, child_visible, structure);
            }
        }
        serde_json::Value::String(text) => {
            if in_visible_output && !text.trim().is_empty() {
                structure.has_final_answer = true;
                structure.has_output_text = true;
            }
        }
        _ => {}
    }
}

fn mark_text_if_present(value: Option<&serde_json::Value>, structure: &mut CodexResponseStructure) {
    if value
        .and_then(serde_json::Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
    {
        structure.has_final_answer = true;
        structure.has_output_text = true;
    }
}

fn is_tool_call_type(type_name: &str) -> bool {
    matches!(
        type_name,
        "function_call"
            | "tool_call"
            | "web_search_call"
            | "computer_call"
            | "local_shell_call"
            | "mcp_call"
            | "code_interpreter_call"
    ) || type_name.ends_with("_tool_call")
}

fn is_reasoning_item_type(type_name: &str) -> bool {
    matches!(type_name, "reasoning" | "reasoning_item")
}

fn is_final_answer_only_intercept_effort(effort: Option<&str>) -> bool {
    matches!(effort, Some("high" | "xhigh"))
}

fn is_context_compaction_exempt(
    request_kind: CodexRequestKind,
    reasoning_tokens: Option<i64>,
) -> bool {
    request_kind == CodexRequestKind::ContextCompaction && reasoning_tokens == Some(0)
}

fn should_intercept_final_answer_only_reasoning(reasoning_tokens: Option<i64>) -> bool {
    reasoning_tokens != Some(0)
}

fn matches_formula_518n_minus_2(reasoning_tokens: i64) -> bool {
    reasoning_tokens >= 516 && (reasoning_tokens + 2) % 518 == 0
}

fn resolve_guard_rule<'a>(
    requested_model: Option<&str>,
    fallback_compare_mode: CodexReasoningGuardCompareMode,
    fallback_values: &'a [i64],
    model_rules: &'a [CodexReasoningGuardModelRule],
) -> Option<ResolvedCodexReasoningGuardRule<'a>> {
    let requested_model = requested_model
        .map(str::trim)
        .filter(|model| !model.is_empty());
    if let Some(requested_model) = requested_model {
        if let Some(rule) = model_rules
            .iter()
            .find(|rule| rule.requested_model == requested_model)
        {
            if !rule.reasoning_equals.is_empty() {
                return Some(ResolvedCodexReasoningGuardRule {
                    compare_mode: rule.compare_mode,
                    configured_values: &rule.reasoning_equals,
                    rule_source: CODEX_REASONING_GUARD_RULE_SOURCE_MODEL_RULE,
                    rule_model: Some(rule.requested_model.as_str()),
                });
            }
        }
    }
    if fallback_values.is_empty() {
        return None;
    }
    Some(ResolvedCodexReasoningGuardRule {
        compare_mode: fallback_compare_mode,
        configured_values: fallback_values,
        rule_source: CODEX_REASONING_GUARD_RULE_SOURCE_GLOBAL_DEFAULT,
        rule_model: None,
    })
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

pub(super) fn push_check_special_setting(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    provider_id: i64,
    provider_name: &str,
    retry_index: u32,
    evaluation: &CodexReasoningGuardEvaluation,
) {
    let structure = &evaluation.observation.structure;
    response_fixer::push_special_setting(
        special_settings,
        serde_json::json!({
            "type": "codex_reasoning_guard_check",
            "scope": "attempt",
            "checked": evaluation.checked,
            "matched": evaluation.matched.is_some(),
            "ruleMode": evaluation.rule_mode,
            "reasoningMatchMode": evaluation.match_mode,
            "providerId": provider_id,
            "providerName": provider_name,
            "retryAttemptNumber": retry_index,
            "reasoningTokens": evaluation.observation.reasoning_tokens,
            "pointer": evaluation.observation.reasoning_pointer,
            "reasoningEffort": evaluation.reasoning_effort,
            "requestedModel": evaluation.requested_model,
            "hasFinalAnswer": structure.has_final_answer,
            "finalAnswerOnly": structure.final_answer_only(),
            "commentaryObserved": structure.has_commentary,
            "hasToolCall": structure.has_tool_call,
            "hasReasoningItem": structure.has_reasoning_item,
            "requestKind": evaluation.request_kind.as_str(),
            "interceptExemptReason": evaluation.intercept_exempt_reason,
            "missReason": evaluation.miss_reason,
        }),
    );
}

pub(super) fn is_responses_path(path: &str) -> bool {
    matches!(path.trim_end_matches('/'), "/responses" | "/v1/responses")
}

pub(super) fn should_strip_encrypted_content_from_continuation_response(
    cli_key: &str,
    path: &str,
    stream_action: CodexReasoningGuardStreamAction,
    body: &[u8],
) -> bool {
    cli_key == "codex"
        && is_responses_path(path)
        && stream_action == CodexReasoningGuardStreamAction::ContinuationRecovery
        && serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|value| value.get("stream").and_then(serde_json::Value::as_bool))
            == Some(true)
}

pub(super) fn build_continuation_recovery_body(
    base_body: &[u8],
    marker_text: &str,
) -> Option<Vec<u8>> {
    let base_json = serde_json::from_slice::<serde_json::Value>(base_body).ok()?;

    let mut next_body = base_json
        .as_object()
        .cloned()
        .map(serde_json::Value::Object)
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(object) = next_body.as_object_mut() {
        object.remove("previous_response_id");
        remove_continuation_encrypted_include(object);
    }
    next_body["stream"] = serde_json::Value::Bool(true);
    next_body["input"] = serde_json::Value::Array(
        normalize_responses_input_for_continuation(base_json.get("input"))
            .into_iter()
            .chain(std::iter::once(build_continuation_marker_item(marker_text)))
            .collect(),
    );
    serde_json::to_vec(&next_body).ok()
}

pub(super) fn strip_encrypted_content_from_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                strip_encrypted_content_from_json(item);
            }
        }
        serde_json::Value::Object(object) => {
            object.remove("encrypted_content");
            for item in object.values_mut() {
                strip_encrypted_content_from_json(item);
            }
        }
        _ => {}
    }
}

fn normalize_escaped_encrypted_content_key(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let mut slash_count = 1usize;
        while chars.peek() == Some(&'\\') {
            slash_count += 1;
            chars.next();
        }

        if chars
            .peek()
            .is_some_and(|next| *next == 'u' || *next == 'U')
        {
            chars.next();
            let mut hex = String::with_capacity(4);
            for _ in 0..4 {
                let Some(next) = chars.peek().copied() else {
                    break;
                };
                if next.is_ascii_hexdigit() {
                    hex.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            if hex.len() == 4 {
                if let Ok(codepoint) = u32::from_str_radix(&hex, 16) {
                    if (0x20..=0x7e).contains(&codepoint) {
                        output.push(char::from_u32(codepoint).unwrap_or('?'));
                        continue;
                    }
                }
            }
            output.push_str(&"\\".repeat(slash_count));
            output.push('u');
            output.push_str(&hex);
            continue;
        }

        output.push_str(&"\\".repeat(slash_count));
    }
    output
}

fn text_may_contain_encrypted_content(text: &str) -> bool {
    normalize_escaped_encrypted_content_key(text).contains("encrypted_content")
}

fn redact_encrypted_content_text(text: &str) -> String {
    let normalized = normalize_escaped_encrypted_content_key(text);
    if !normalized.contains("encrypted_content") {
        return text.to_string();
    }
    normalized.replace("encrypted_content", "redacted_sensitive_content")
}

pub(super) fn strip_encrypted_content_from_sse(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    if !text_may_contain_encrypted_content(&text) {
        return raw.to_vec();
    }

    let normalized = text.replace("\r\n", "\n");
    let mut output = String::with_capacity(normalized.len());
    for block in normalized.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut event_line: Option<String> = None;
        let mut data_lines = Vec::new();
        let mut passthrough = Vec::new();
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("event:") {
                event_line = Some(format!("event:{}", rest));
            } else if let Some(rest) = line.strip_prefix("data:") {
                data_lines.push(rest.trim_start());
            } else {
                passthrough.push(line.to_string());
            }
        }
        let sanitized_passthrough = passthrough
            .iter()
            .map(|line| redact_encrypted_content_text(line))
            .collect::<Vec<_>>();

        if data_lines.is_empty() {
            for line in sanitized_passthrough {
                output.push_str(&line);
                output.push('\n');
            }
            output.push('\n');
            continue;
        }
        let data_text = data_lines.join("\n");
        if data_text == "[DONE]" {
            if let Some(event) = event_line {
                output.push_str(&event);
                output.push('\n');
            }
            output.push_str("data: [DONE]\n\n");
            continue;
        }
        match serde_json::from_str::<serde_json::Value>(&data_text) {
            Ok(mut value) => {
                strip_encrypted_content_from_json(&mut value);
                if let Some(event) = event_line {
                    output.push_str(&event);
                    output.push('\n');
                }
                for line in &sanitized_passthrough {
                    output.push_str(line);
                    output.push('\n');
                }
                output.push_str("data: ");
                output.push_str(&serde_json::to_string(&value).unwrap_or(data_text));
                output.push_str("\n\n");
            }
            Err(_) => {
                if text_may_contain_encrypted_content(&data_text) {
                    if let Some(event) = event_line {
                        output.push_str(&event);
                        output.push('\n');
                    }
                    for line in &sanitized_passthrough {
                        output.push_str(line);
                        output.push('\n');
                    }
                    output.push_str("data: {\"type\":\"gateway.redacted\",\"redacted\":true}\n\n");
                } else {
                    output.push_str(&redact_encrypted_content_text(block));
                    output.push_str("\n\n");
                }
            }
        }
    }
    output.into_bytes()
}

fn remove_continuation_encrypted_include(object: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(include) = object
        .get("include")
        .and_then(serde_json::Value::as_array)
        .cloned()
    else {
        return;
    };
    let include = include
        .into_iter()
        .filter(|item| item.as_str() != Some(CONTINUATION_ENCRYPTED_INCLUDE))
        .collect::<Vec<_>>();
    if include.is_empty() {
        object.remove("include");
    } else {
        object.insert("include".to_string(), serde_json::Value::Array(include));
    }
}

fn normalize_responses_input_for_continuation(
    value: Option<&serde_json::Value>,
) -> Vec<serde_json::Value> {
    match value {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(normalize_responses_input_item_for_continuation)
            .collect(),
        Some(value) => normalize_responses_input_item_for_continuation(value)
            .into_iter()
            .collect(),
        None => Vec::new(),
    }
}

fn normalize_responses_input_item_for_continuation(
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    if let Some(text) = value.as_str() {
        return Some(serde_json::json!({
            "type": "message",
            "role": "user",
            "content": text,
        }));
    }
    if value.get("type").and_then(serde_json::Value::as_str) == Some("reasoning") {
        return None;
    }
    let mut value = value.clone();
    strip_encrypted_content_from_json(&mut value);
    Some(value)
}

fn build_continuation_marker_item(marker_text: &str) -> serde_json::Value {
    let text = marker_text.trim();
    let text = if text.is_empty() {
        crate::settings::DEFAULT_CODEX_REASONING_GUARD_CONTINUATION_MARKER_TEXT
    } else {
        text
    };
    serde_json::json!({
        "type": "message",
        "role": "assistant",
        "content": [{
            "type": "output_text",
            "text": text,
            "channel": "commentary",
        }],
    })
}

pub(super) fn push_special_setting(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    provider_id: i64,
    provider_name: &str,
    retry_index: u32,
    matched: &CodexReasoningGuardMatch,
    budget: CodexReasoningGuardBudgetDecision,
) {
    response_fixer::push_special_setting(
        special_settings,
        serde_json::json!({
            "type": "codex_reasoning_guard",
            "scope": "attempt",
            "hit": true,
            "ruleMode": matched.rule_mode,
            "reasoningMatchMode": matched.match_mode,
            "providerId": provider_id,
            "providerName": provider_name,
            "reasoningTokens": matched.reasoning_tokens,
            "finalAnswerOnly": matched.final_answer_only,
            "commentaryObserved": matched.commentary_observed,
            "hasToolCall": matched.has_tool_call,
            "hasReasoningItem": matched.has_reasoning_item,
            "requestKind": matched.request_kind.as_str(),
            "interceptExemptReason": matched.intercept_exempt_reason,
            "reasoningEffort": matched.reasoning_effort,
            "compareMode": matched.compare_mode,
            "compareModeSymbol": compare_mode_symbol(matched.compare_mode),
            "matchedRuleValue": matched.matched_rule_value,
            "pointer": matched.pointer,
            "requestedModel": matched.requested_model,
            "ruleSource": matched.rule_source,
            "ruleModel": matched.rule_model,
            "retryAttemptNumber": retry_index,
            "retryAttemptNumberNext": retry_index.saturating_add(1),
            "displayStatus": StatusCode::BAD_GATEWAY.as_u16(),
            "action": budget.action_taken,
            "actionTaken": budget.action_taken,
            "backoffApplied": budget.delay_ms > 0,
            "backoffAfterHits": budget.immediate_budget,
            "backoffMs": budget.delay_ms,
            "guardHitNumber": budget.hit_number,
            "guardRetryPhase": budget.phase,
            "guardBudgetRemaining": budget.remaining_budget,
            "guardBudgetTotal": budget.total_budget,
            "guardExhaustedAction": budget.exhausted_action,
        }),
    );
}

pub(super) fn push_continuation_recovery_setting(
    special_settings: &Arc<Mutex<Vec<serde_json::Value>>>,
    provider_id: i64,
    provider_name: &str,
    retry_index: u32,
    action: &str,
    count: u32,
    success_count: u32,
) {
    let success_ratio = if count == 0 {
        0.0
    } else {
        success_count as f64 / count as f64
    };
    response_fixer::push_special_setting(
        special_settings,
        serde_json::json!({
            "type": "codex_continuation_recovery",
            "scope": "attempt",
            "providerId": provider_id,
            "providerName": provider_name,
            "retryAttemptNumber": retry_index,
            "action": action,
            "streamAction": CodexReasoningGuardStreamAction::ContinuationRecovery,
            "continuationRecoveryCount": count,
            "continuationRecoverySuccessCount": success_count,
            "continuationRecoverySuccessRatio": success_ratio,
        }),
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodexReasoningGuardBudgetAction {
    RetrySameProvider,
    ReturnError,
    SwitchProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CodexReasoningGuardBudgetDecision {
    pub(super) action: CodexReasoningGuardBudgetAction,
    pub(super) hit_number: u32,
    pub(super) phase: &'static str,
    pub(super) delay_ms: u32,
    pub(super) immediate_budget: u32,
    pub(super) delayed_budget: u32,
    pub(super) total_budget: u32,
    pub(super) remaining_budget: u32,
    pub(super) exhausted_action: &'static str,
    pub(super) action_taken: &'static str,
}

#[cfg(test)]
pub(super) fn budget_decision(
    current_hits: u32,
    immediate_budget: u32,
    delayed_budget: u32,
    delayed_retry_ms: u32,
    exhausted_action: CodexReasoningGuardExhaustedAction,
) -> CodexReasoningGuardBudgetDecision {
    let hit_number = current_hits.saturating_add(1);
    let total_budget = immediate_budget.saturating_add(delayed_budget);
    let exhausted_action_label = match exhausted_action {
        CodexReasoningGuardExhaustedAction::ReturnError => "return_error",
        CodexReasoningGuardExhaustedAction::SwitchProvider => "switch_provider",
    };

    if hit_number <= immediate_budget {
        return CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::RetrySameProvider,
            hit_number,
            phase: "immediate",
            delay_ms: 0,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: total_budget.saturating_sub(hit_number),
            exhausted_action: exhausted_action_label,
            action_taken: "retry_same_provider_no_circuit",
        };
    }

    if hit_number <= total_budget {
        return CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::RetrySameProvider,
            hit_number,
            phase: "delayed",
            delay_ms: delayed_retry_ms,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: total_budget.saturating_sub(hit_number),
            exhausted_action: exhausted_action_label,
            action_taken: "retry_same_provider_delayed_no_circuit",
        };
    }

    match exhausted_action {
        CodexReasoningGuardExhaustedAction::ReturnError => CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::ReturnError,
            hit_number,
            phase: "exhausted",
            delay_ms: 0,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: 0,
            exhausted_action: exhausted_action_label,
            action_taken: "return_guard_error_no_circuit",
        },
        CodexReasoningGuardExhaustedAction::SwitchProvider => CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::SwitchProvider,
            hit_number,
            phase: "exhausted",
            delay_ms: 0,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: 0,
            exhausted_action: exhausted_action_label,
            action_taken: "switch_provider_no_circuit",
        },
    }
}

pub(super) fn shared_budget_decision(
    state: &Arc<Mutex<super::layered_policy::LayeredPolicyState>>,
    immediate_budget: u32,
    delayed_budget: u32,
    exhausted_action: CodexReasoningGuardExhaustedAction,
) -> CodexReasoningGuardBudgetDecision {
    let total_budget = immediate_budget.saturating_add(delayed_budget);
    let exhausted_action_label = match exhausted_action {
        CodexReasoningGuardExhaustedAction::ReturnError => "return_error",
        CodexReasoningGuardExhaustedAction::SwitchProvider => "switch_provider",
    };
    if let Some(reservation) = super::layered_policy::reserve_shared_retry(state) {
        return CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::RetrySameProvider,
            hit_number: reservation.used,
            phase: reservation.phase,
            delay_ms: reservation.budget_delay_ms,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: reservation.remaining,
            exhausted_action: exhausted_action_label,
            action_taken: if reservation.budget_delay_ms == 0 {
                "retry_same_provider_no_circuit"
            } else {
                "retry_same_provider_delayed_no_circuit"
            },
        };
    }

    let (used, _) = super::layered_policy::budget_snapshot(state);
    let hit_number = used.saturating_add(1);
    match exhausted_action {
        CodexReasoningGuardExhaustedAction::ReturnError => CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::ReturnError,
            hit_number,
            phase: "exhausted",
            delay_ms: 0,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: 0,
            exhausted_action: exhausted_action_label,
            action_taken: "return_guard_error_no_circuit",
        },
        CodexReasoningGuardExhaustedAction::SwitchProvider => CodexReasoningGuardBudgetDecision {
            action: CodexReasoningGuardBudgetAction::SwitchProvider,
            hit_number,
            phase: "exhausted",
            delay_ms: 0,
            immediate_budget,
            delayed_budget,
            total_budget,
            remaining_budget: 0,
            exhausted_action: exhausted_action_label,
            action_taken: "switch_provider_no_circuit",
        },
    }
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
    budget: CodexReasoningGuardBudgetDecision,
) {
    let (outcome, decision) = match budget.action {
        CodexReasoningGuardBudgetAction::RetrySameProvider => {
            ("codex_reasoning_guard_retry", "retry_same_provider")
        }
        CodexReasoningGuardBudgetAction::ReturnError => {
            ("codex_reasoning_guard_exhausted", "abort")
        }
        CodexReasoningGuardBudgetAction::SwitchProvider => {
            ("codex_reasoning_guard_switch_provider", "switch")
        }
    };
    attempts.push(FailoverAttempt {
        provider_id,
        provider_name: provider_name.to_string(),
        base_url: base_url.to_string(),
        outcome: outcome.to_string(),
        status: Some(StatusCode::BAD_GATEWAY.as_u16()),
        provider_index: Some(provider_index),
        retry_index: Some(retry_index),
        session_reuse,
        error_category: Some(ErrorCategory::SystemError.as_str()),
        error_code: Some(CODEX_REASONING_GUARD_ERROR_CODE),
        decision: Some(decision),
        reason: Some(guard_attempt_reason(matched, budget)),
        selection_method: dc::selection_method(provider_index, retry_index, session_reuse),
        reason_code: Some(CODEX_REASONING_GUARD_REASON_CODE),
        attempt_started_ms: Some(attempt_started_ms),
        attempt_duration_ms: Some(attempt_duration_ms),
        circuit_state_before: Some(circuit_state_before),
        circuit_state_after: Some(circuit_state_before),
        circuit_failure_count: Some(circuit_failure_count),
        circuit_failure_threshold: Some(circuit_failure_threshold),
        circuit_recover_at_unix: None,
        circuit_trigger_error_code: None,
        provider_bridged: None,
        timeout_secs: None,
    });
}

fn guard_attempt_reason(
    matched: &CodexReasoningGuardMatch,
    budget: CodexReasoningGuardBudgetDecision,
) -> String {
    match matched.rule_mode {
        CodexReasoningGuardRuleMode::ReasoningTokens => format!(
            "codex reasoning guard matched reasoning_tokens={} {} {} via {} ({}) hit={} phase={} action={}",
            matched
                .reasoning_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string()),
            compare_mode_symbol(matched.compare_mode),
            matched
                .matched_rule_value
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string()),
            matched.pointer.unwrap_or("unknown"),
            matched.rule_source,
            budget.hit_number,
            budget.phase,
            budget.action_taken
        ),
        CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh => format!(
            "codex reasoning guard matched final-answer-only effort={} hit={} phase={} action={}",
            matched
                .reasoning_effort
                .as_deref()
                .unwrap_or("unknown"),
            budget.hit_number,
            budget.phase,
            budget.action_taken
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection_options<'a>(
        rule_mode: CodexReasoningGuardRuleMode,
        match_mode: CodexReasoningGuardMatchMode,
        request_kind: CodexRequestKind,
        requested_reasoning_effort: Option<&'a str>,
        fallback_compare_mode: CodexReasoningGuardCompareMode,
        fallback_values: &'a [i64],
        model_rules: &'a [CodexReasoningGuardModelRule],
    ) -> CodexReasoningGuardDetectionOptions<'a> {
        CodexReasoningGuardDetectionOptions {
            rule_mode,
            match_mode,
            request_kind,
            requested_reasoning_effort,
            fallback_compare_mode,
            fallback_values,
            model_rules,
        }
    }

    fn detect_token_mode(
        cli_key: &str,
        requested_model: Option<&str>,
        value: &serde_json::Value,
        fallback_compare_mode: CodexReasoningGuardCompareMode,
        fallback_values: &[i64],
        model_rules: &[CodexReasoningGuardModelRule],
    ) -> Option<CodexReasoningGuardMatch> {
        detect_from_json(
            cli_key,
            requested_model,
            value,
            detection_options(
                CodexReasoningGuardRuleMode::ReasoningTokens,
                CodexReasoningGuardMatchMode::Manual,
                CodexRequestKind::Normal,
                None,
                fallback_compare_mode,
                fallback_values,
                model_rules,
            ),
        )
    }

    #[test]
    fn detect_from_json_matches_equals_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 516 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516, 1024],
            &[],
        )
        .expect("should match");

        assert_eq!(matched.reasoning_tokens, Some(516));
        assert_eq!(matched.matched_rule_value, Some(516));
        assert_eq!(matched.compare_mode, CodexReasoningGuardCompareMode::Equals);
        assert_eq!(
            matched.rule_source,
            CODEX_REASONING_GUARD_RULE_SOURCE_GLOBAL_DEFAULT
        );
    }

    #[test]
    fn budget_decision_uses_immediate_then_delayed_budget() {
        for current_hits in 0..5 {
            let decision = budget_decision(
                current_hits,
                5,
                5,
                1_000,
                CodexReasoningGuardExhaustedAction::ReturnError,
            );
            assert_eq!(
                decision.action,
                CodexReasoningGuardBudgetAction::RetrySameProvider
            );
            assert_eq!(decision.phase, "immediate");
            assert_eq!(decision.delay_ms, 0);
            assert_eq!(decision.remaining_budget, 9 - current_hits);
        }

        for current_hits in 5..10 {
            let decision = budget_decision(
                current_hits,
                5,
                5,
                1_000,
                CodexReasoningGuardExhaustedAction::ReturnError,
            );
            assert_eq!(
                decision.action,
                CodexReasoningGuardBudgetAction::RetrySameProvider
            );
            assert_eq!(decision.phase, "delayed");
            assert_eq!(decision.delay_ms, 1_000);
            assert_eq!(decision.remaining_budget, 9 - current_hits);
        }
    }

    #[test]
    fn budget_decision_exhausts_to_configured_terminal_action() {
        let return_error = budget_decision(
            10,
            5,
            5,
            1_000,
            CodexReasoningGuardExhaustedAction::ReturnError,
        );
        assert_eq!(
            return_error.action,
            CodexReasoningGuardBudgetAction::ReturnError
        );
        assert_eq!(return_error.phase, "exhausted");
        assert_eq!(return_error.remaining_budget, 0);

        let switch_provider = budget_decision(
            10,
            5,
            5,
            1_000,
            CodexReasoningGuardExhaustedAction::SwitchProvider,
        );
        assert_eq!(
            switch_provider.action,
            CodexReasoningGuardBudgetAction::SwitchProvider
        );
        assert_eq!(switch_provider.exhausted_action, "switch_provider");
    }

    #[test]
    fn budget_decision_supports_zero_budget_edges() {
        let delayed_first = budget_decision(
            0,
            0,
            1,
            500,
            CodexReasoningGuardExhaustedAction::ReturnError,
        );
        assert_eq!(
            delayed_first.action,
            CodexReasoningGuardBudgetAction::RetrySameProvider
        );
        assert_eq!(delayed_first.phase, "delayed");
        assert_eq!(delayed_first.delay_ms, 500);

        let exhausted_first = budget_decision(
            0,
            0,
            0,
            500,
            CodexReasoningGuardExhaustedAction::ReturnError,
        );
        assert_eq!(
            exhausted_first.action,
            CodexReasoningGuardBudgetAction::ReturnError
        );

        let immediate_only = budget_decision(
            1,
            2,
            0,
            500,
            CodexReasoningGuardExhaustedAction::ReturnError,
        );
        assert_eq!(
            immediate_only.action,
            CodexReasoningGuardBudgetAction::RetrySameProvider
        );
        assert_eq!(immediate_only.phase, "immediate");
        assert_eq!(immediate_only.remaining_budget, 0);

        let exhausted_after_immediate = budget_decision(
            2,
            2,
            0,
            500,
            CodexReasoningGuardExhaustedAction::ReturnError,
        );
        assert_eq!(
            exhausted_after_immediate.action,
            CodexReasoningGuardBudgetAction::ReturnError
        );
        assert_eq!(exhausted_after_immediate.phase, "exhausted");
    }

    #[test]
    fn detect_from_json_does_not_match_equals_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 300 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516],
            &[],
        );

        assert!(matched.is_none());
    }

    #[test]
    fn evaluate_from_json_explains_token_rule_miss() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 700 } }
        });

        let evaluation = evaluate_from_json(
            "codex",
            Some("gpt-5.5"),
            &value,
            detection_options(
                CodexReasoningGuardRuleMode::ReasoningTokens,
                CodexReasoningGuardMatchMode::Formula518nMinus2,
                CodexRequestKind::Normal,
                Some("xhigh"),
                CodexReasoningGuardCompareMode::Equals,
                &[516],
                &[],
            ),
        );

        assert!(evaluation.checked);
        assert!(evaluation.matched.is_none());
        assert_eq!(
            evaluation.miss_reason,
            Some("reasoning_tokens_not_formula_match")
        );
        assert_eq!(evaluation.observation.reasoning_tokens, Some(700));
        assert_eq!(evaluation.reasoning_effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn detect_from_json_matches_less_than_or_equal_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 300 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[516],
            &[],
        )
        .expect("should match");

        assert_eq!(matched.reasoning_tokens, Some(300));
        assert_eq!(matched.matched_rule_value, Some(516));
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

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[1024, 516, 2048],
            &[],
        )
        .expect("should match");

        assert_eq!(matched.matched_rule_value, Some(516));
    }

    #[test]
    fn detect_from_json_does_not_match_less_than_or_equal_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 800 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::LessThanOrEqual,
            &[516],
            &[],
        );

        assert!(matched.is_none());
    }

    #[test]
    fn detect_from_json_prefers_exact_model_rule() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 600 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-codex"),
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516],
            &[CodexReasoningGuardModelRule {
                requested_model: "gpt-5-codex".to_string(),
                compare_mode: CodexReasoningGuardCompareMode::LessThanOrEqual,
                reasoning_equals: vec![700],
            }],
        )
        .expect("should match model rule");

        assert_eq!(matched.matched_rule_value, Some(700));
        assert_eq!(
            matched.rule_source,
            CODEX_REASONING_GUARD_RULE_SOURCE_MODEL_RULE
        );
        assert_eq!(matched.rule_model.as_deref(), Some("gpt-5-codex"));
    }

    #[test]
    fn detect_from_json_falls_back_to_global_rule_when_model_rule_missing() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 516 } }
        });

        let matched = detect_token_mode(
            "codex",
            Some("gpt-5-mini-codex"),
            &value,
            CodexReasoningGuardCompareMode::Equals,
            &[516],
            &[CodexReasoningGuardModelRule {
                requested_model: "gpt-5-codex".to_string(),
                compare_mode: CodexReasoningGuardCompareMode::LessThanOrEqual,
                reasoning_equals: vec![700],
            }],
        )
        .expect("should fall back to global rule");

        assert_eq!(matched.matched_rule_value, Some(516));
        assert_eq!(
            matched.rule_source,
            CODEX_REASONING_GUARD_RULE_SOURCE_GLOBAL_DEFAULT
        );
        assert!(matched.rule_model.is_none());
    }

    #[test]
    fn final_answer_only_mode_matches_high_and_xhigh_without_reasoning_tokens() {
        let value = serde_json::json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        for effort in ["high", "xhigh"] {
            let matched = detect_from_json(
                "codex",
                Some("gpt-5.5"),
                &value,
                detection_options(
                    CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
                    CodexReasoningGuardMatchMode::Manual,
                    CodexRequestKind::Normal,
                    Some(effort),
                    CodexReasoningGuardCompareMode::Equals,
                    &[516],
                    &[],
                ),
            )
            .expect("final-only high/xhigh should match");

            assert_eq!(
                matched.rule_mode,
                CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh
            );
            assert_eq!(matched.reasoning_tokens, None);
            assert_eq!(matched.matched_rule_value, None);
            assert!(matched.final_answer_only);
            assert_eq!(matched.reasoning_effort.as_deref(), Some(effort));
        }
    }

    #[test]
    fn final_answer_only_mode_ignores_efforts_other_than_high_xhigh() {
        let value = serde_json::json!({
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        for effort in [
            Some("minimal"),
            Some("low"),
            Some("medium"),
            Some("max"),
            Some("ultra"),
            None,
        ] {
            let matched = detect_from_json(
                "codex",
                Some("gpt-5.5"),
                &value,
                detection_options(
                    CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
                    CodexReasoningGuardMatchMode::Manual,
                    CodexRequestKind::Normal,
                    effort,
                    CodexReasoningGuardCompareMode::Equals,
                    &[516],
                    &[],
                ),
            );
            assert!(matched.is_none());
        }
    }

    #[test]
    fn final_answer_only_mode_rejects_commentary_tool_call_and_reasoning_item() {
        let base_message = serde_json::json!({
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "final answer" }]
        });
        let cases = [
            serde_json::json!({ "output": [base_message.clone(), { "type": "message", "channel": "commentary", "content": [{ "type": "output_text", "text": "note" }] }] }),
            serde_json::json!({ "output": [base_message.clone(), { "type": "function_call", "name": "do_work" }] }),
            serde_json::json!({ "output": [base_message, { "type": "reasoning", "summary": [] }] }),
        ];

        for value in cases {
            let matched = detect_from_json(
                "codex",
                Some("gpt-5.5"),
                &value,
                detection_options(
                    CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
                    CodexReasoningGuardMatchMode::Manual,
                    CodexRequestKind::Normal,
                    Some("high"),
                    CodexReasoningGuardCompareMode::Equals,
                    &[516],
                    &[],
                ),
            );
            assert!(matched.is_none());
        }
    }

    #[test]
    fn context_compaction_with_zero_reasoning_is_exempt_in_both_rule_modes() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 0 } },
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        for rule_mode in [
            CodexReasoningGuardRuleMode::ReasoningTokens,
            CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
        ] {
            let matched = detect_from_json(
                "codex",
                Some("gpt-5.5"),
                &value,
                detection_options(
                    rule_mode,
                    CodexReasoningGuardMatchMode::Manual,
                    CodexRequestKind::ContextCompaction,
                    Some("high"),
                    CodexReasoningGuardCompareMode::Equals,
                    &[516],
                    &[],
                ),
            );
            assert!(matched.is_none());
        }
    }

    #[test]
    fn evaluate_from_json_records_context_compaction_exemption() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 0 } }
        });
        let evaluation = evaluate_from_json(
            "codex",
            Some("gpt-5.5"),
            &value,
            detection_options(
                CodexReasoningGuardRuleMode::ReasoningTokens,
                CodexReasoningGuardMatchMode::Manual,
                CodexRequestKind::ContextCompaction,
                Some("high"),
                CodexReasoningGuardCompareMode::Equals,
                &[516],
                &[],
            ),
        );

        assert!(evaluation.checked);
        assert!(evaluation.matched.is_none());
        assert_eq!(evaluation.miss_reason, Some("context_compaction_exempt"));
        assert_eq!(
            evaluation.intercept_exempt_reason,
            Some("context_compaction")
        );
    }

    #[test]
    fn context_compaction_with_nonzero_reasoning_still_matches_token_mode() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 516 } },
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        let matched = detect_from_json(
            "codex",
            Some("gpt-5.5"),
            &value,
            detection_options(
                CodexReasoningGuardRuleMode::ReasoningTokens,
                CodexReasoningGuardMatchMode::Manual,
                CodexRequestKind::ContextCompaction,
                Some("high"),
                CodexReasoningGuardCompareMode::Equals,
                &[516],
                &[],
            ),
        )
        .expect("nonzero compaction reasoning should still match token mode");

        assert_eq!(matched.reasoning_tokens, Some(516));
        assert_eq!(matched.request_kind, CodexRequestKind::ContextCompaction);
    }

    #[test]
    fn final_answer_only_mode_excludes_zero_reasoning_tokens() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 0 } },
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        let matched = detect_from_json(
            "codex",
            Some("gpt-5.5"),
            &value,
            detection_options(
                CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
                CodexReasoningGuardMatchMode::Manual,
                CodexRequestKind::Normal,
                Some("high"),
                CodexReasoningGuardCompareMode::Equals,
                &[516],
                &[],
            ),
        );

        assert!(matched.is_none());
    }

    #[test]
    fn final_answer_only_mode_allows_positive_reasoning_tokens() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": 18 } },
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        let matched = detect_from_json(
            "codex",
            Some("gpt-5.5"),
            &value,
            detection_options(
                CodexReasoningGuardRuleMode::FinalAnswerOnlyHighXhigh,
                CodexReasoningGuardMatchMode::Manual,
                CodexRequestKind::Normal,
                Some("high"),
                CodexReasoningGuardCompareMode::Equals,
                &[516],
                &[],
            ),
        )
        .expect("positive final-only reasoning should match");

        assert_eq!(matched.reasoning_tokens, Some(18));
    }

    #[test]
    fn observe_response_reads_string_reasoning_tokens_and_structure() {
        let value = serde_json::json!({
            "usage": { "output_tokens_details": { "reasoning_tokens": "516" } },
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{ "type": "output_text", "text": "final answer" }]
            }]
        });

        let observation = observe_response(&value);

        assert_eq!(observation.reasoning_tokens, Some(516));
        assert!(observation.structure.has_output_text);
        assert!(observation.structure.final_answer_only());
    }

    #[test]
    fn continuation_recovery_body_replays_original_input_without_encrypted_reasoning() {
        let base = serde_json::json!({
            "model": "gpt-5.4",
            "stream": true,
            "previous_response_id": "resp_old",
            "include": ["reasoning.encrypted_content", "file_search_call.results"],
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": "hello",
                    "encrypted_content": "secret"
                },
                {
                    "type": "reasoning",
                    "encrypted_content": "reasoning-secret"
                }
            ]
        });
        let body = build_continuation_recovery_body(
            serde_json::to_vec(&base).unwrap().as_slice(),
            "Continue thinking...",
        )
        .expect("continuation body");
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(value.get("previous_response_id").is_none());
        assert_eq!(
            value.get("include").unwrap(),
            &serde_json::json!(["file_search_call.results"])
        );
        let input = value
            .get("input")
            .and_then(serde_json::Value::as_array)
            .unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(
            input[0].get("type").and_then(serde_json::Value::as_str),
            Some("message")
        );
        assert!(input[0].get("encrypted_content").is_none());
        assert_eq!(
            input[1]
                .pointer("/content/0/channel")
                .and_then(serde_json::Value::as_str),
            Some("commentary")
        );
    }

    #[test]
    fn strip_encrypted_content_from_sse_redacts_malformed_payloads() {
        let raw = b"event: response.output_item.done\ndata: {\"type\":\"reasoning\",\"encrypted_content\":\"secret\"\n\n\
data: [DONE]\n\n";

        let stripped = String::from_utf8(strip_encrypted_content_from_sse(raw)).unwrap();

        assert!(!stripped.contains("encrypted_content"));
        assert!(!stripped.contains("secret"));
        assert!(stripped.contains("gateway.redacted"));
        assert!(stripped.contains("data: [DONE]"));
    }

    #[test]
    fn continuation_response_stripping_is_enabled_for_streaming_responses() {
        let body = br#"{"stream":true,"include":["reasoning.encrypted_content"]}"#;

        assert!(should_strip_encrypted_content_from_continuation_response(
            "codex",
            "/v1/responses",
            CodexReasoningGuardStreamAction::ContinuationRecovery,
            body,
        ));
        assert!(!should_strip_encrypted_content_from_continuation_response(
            "codex",
            "/v1/responses",
            CodexReasoningGuardStreamAction::Strict502,
            body,
        ));
    }
}
