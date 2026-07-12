import { describe, expect, it } from "vitest";
import {
  countCodexReasoningGuardSpecialSettings,
  formatCodexReasoningEffortSource,
  hasClaudeModelMappingSpecialSetting,
  parseRequestLogSpecialSettings,
  resolveClaudeModelMappingFromSpecialSettings,
  resolveCodexReasoningEffort,
  resolveCodexReasoningGuardCheckSummary,
  resolveCodexReasoningGuardSummary,
} from "../requestLogSpecialSettings";

describe("services/gateway/requestLogSpecialSettings", () => {
  it("resolves Claude model mapping with final provider preference", () => {
    const settings = JSON.stringify([
      { type: "noop" },
      {
        type: "claude_model_mapping",
        requestedModel: " claude-sonnet ",
        effectiveModel: " gpt-4.1 ",
        mappingKind: " sonnet ",
        providerId: 1,
        providerName: " Provider A ",
        applied: true,
      },
      {
        type: "claude_model_mapping",
        requestedModel: " claude-sonnet ",
        effectiveModel: " gpt-5.4 ",
        mappingKind: " sonnet ",
        providerId: 2,
        providerName: " Provider B ",
        applied: true,
      },
    ]);

    expect(resolveClaudeModelMappingFromSpecialSettings(settings, 2)).toEqual({
      requestedModel: "claude-sonnet",
      effectiveModel: "gpt-5.4",
      mappingKind: "sonnet",
      providerId: 2,
      providerName: "Provider B",
      applied: true,
    });
    expect(resolveClaudeModelMappingFromSpecialSettings(settings, 99)?.providerId).toBe(2);
    expect(hasClaudeModelMappingSpecialSetting(settings)).toBe(true);
  });

  it("ignores invalid, unapplied, and identity mappings", () => {
    expect(resolveClaudeModelMappingFromSpecialSettings(null)).toBeNull();
    expect(resolveClaudeModelMappingFromSpecialSettings("bad-json")).toBeNull();
    expect(
      resolveClaudeModelMappingFromSpecialSettings(
        JSON.stringify([
          {
            type: "claude_model_mapping",
            requestedModel: "same",
            effectiveModel: "same",
            mappingKind: "sonnet",
            providerId: 1,
            providerName: "Provider A",
            applied: true,
          },
          {
            type: "claude_model_mapping",
            requestedModel: "claude-sonnet",
            effectiveModel: "gpt-5.4",
            mappingKind: "sonnet",
            providerId: 2,
            providerName: "Provider B",
            applied: false,
          },
        ])
      )
    ).toBeNull();
    expect(hasClaudeModelMappingSpecialSetting("bad-json")).toBe(false);
  });

  it("parses object and array special settings safely", () => {
    expect(parseRequestLogSpecialSettings(null)).toEqual([]);
    expect(parseRequestLogSpecialSettings("bad-json")).toEqual([]);
    expect(parseRequestLogSpecialSettings(JSON.stringify(1))).toEqual([]);
    expect(parseRequestLogSpecialSettings(JSON.stringify({ type: "single" }))).toEqual([
      { type: "single" },
    ]);
    expect(
      parseRequestLogSpecialSettings(JSON.stringify([null, 1, "text", { type: "valid" }]))
    ).toEqual([{ type: "valid" }]);
  });

  it("resolves explicit and model-default Codex reasoning effort", () => {
    expect(
      resolveCodexReasoningEffort(
        "gpt-5.4",
        JSON.stringify([{ type: "codex_reasoning_effort", effort: " HIGH " }])
      )
    ).toEqual({ effort: "high", source: "request" });
    expect(
      resolveCodexReasoningEffort(
        "gpt-5.4",
        JSON.stringify([{ type: "codex_reasoning_effort", rawEffort: "turbo" }])
      )
    ).toEqual({ effort: "unknown", source: "unknown" });
    expect(
      resolveCodexReasoningEffort(
        " GPT-5.5-PRO ",
        JSON.stringify([{ type: "codex_reasoning_effort" }])
      )
    ).toEqual({ effort: "high", source: "default" });
    expect(resolveCodexReasoningEffort("gpt-5.4-mini", null)).toEqual({
      effort: "none",
      source: "default",
    });
    expect(resolveCodexReasoningEffort("unknown-model", null)).toEqual({
      effort: "unknown",
      source: "unknown",
    });
    expect(resolveCodexReasoningEffort(" ", null)).toEqual({
      effort: "unknown",
      source: "unknown",
    });
    expect(formatCodexReasoningEffortSource("request")).toBe("请求显式");
    expect(formatCodexReasoningEffortSource("default")).toBe("默认推断");
    expect(formatCodexReasoningEffortSource("unknown")).toBe("未知");
  });

  it("summarizes Codex reasoning guard hits and keeps the latest values", () => {
    const settings = JSON.stringify([
      { type: "noop" },
      {
        type: "codex_reasoning_guard",
        ruleMode: "reasoning_tokens",
        compareMode: "equals",
        matchedRuleValue: 516,
        reasoningTokens: 120,
        guardRetryPhase: "immediate",
        action: "retry",
        guardExhaustedAction: "switch_provider",
        backoffMs: 250,
        guardBudgetRemaining: 4,
        guardBudgetTotal: 5,
      },
      {
        type: "codex_reasoning_guard",
        ruleMode: "final_answer_only_high_xhigh",
        compareModeSymbol: "<=",
        matchedRuleValue: 1034,
        reasoningTokens: 300,
        guardRetryPhase: "delayed",
        actionTaken: "continuation_recovery",
        guardExhaustedAction: "return_error",
        backoffMs: 1000,
        guardBudgetRemaining: 1,
        guardBudgetTotal: 2,
      },
    ]);

    expect(resolveCodexReasoningGuardSummary(settings)).toEqual({
      count: 2,
      latestRuleLabel: "final-answer-only / high,xhigh",
      latestReasoningTokens: 300,
      latestPhase: "delayed",
      latestActionTaken: "continuation_recovery",
      latestExhaustedAction: "return_error",
      latestDelayMs: 1000,
      latestBudgetRemaining: 1,
      latestBudgetTotal: 2,
    });
    expect(countCodexReasoningGuardSpecialSettings(settings)).toBe(2);
  });

  it("summarizes Codex reasoning guard checks and explains misses", () => {
    const settings = JSON.stringify([
      {
        type: "codex_reasoning_guard_check",
        checked: true,
        matched: false,
        ruleMode: "reasoning_tokens",
        reasoningMatchMode: "formula_518n_minus_2",
        reasoningTokens: 700,
        reasoningEffort: "high",
        missReason: "reasoning_tokens_not_formula_match",
      },
      {
        type: "codex_reasoning_guard_check",
        checked: true,
        matched: false,
        ruleMode: "final_answer_only_high_xhigh",
        reasoningTokens: null,
        reasoningEffort: "medium",
        missReason: "reasoning_effort_not_high_xhigh",
      },
    ]);

    expect(resolveCodexReasoningGuardCheckSummary(settings)).toEqual({
      count: 2,
      checkedCount: 2,
      matchedCount: 0,
      latestRuleLabel: "final-answer-only / high,xhigh",
      latestReasoningTokens: null,
      latestReasoningEffort: "medium",
      latestMissReason: "reasoning_effort_not_high_xhigh",
      latestMissReasonLabel: "思考等级不是 high/xhigh",
      latestExemptReason: null,
    });
  });

  it("normalizes Codex guard comparison labels and invalid optional values", () => {
    const summarize = (setting: Record<string, unknown>) =>
      resolveCodexReasoningGuardSummary(
        JSON.stringify([{ type: "codex_reasoning_guard", ...setting }])
      );

    expect(summarize({ compareModeSymbol: "==", matchedRuleValue: 516 }).latestRuleLabel).toBe(
      "== 516"
    );
    expect(
      summarize({ compareModeSymbol: "invalid", compareMode: "equals", matchedRuleValue: 516 })
        .latestRuleLabel
    ).toBe("== 516");
    expect(
      summarize({ compareMode: "less_than_or_equal", matchedRuleValue: 1034 }).latestRuleLabel
    ).toBe("<= 1034");
    expect(summarize({ compareMode: "invalid", matchedRuleValue: 1 }).latestRuleLabel).toBeNull();
    expect(
      summarize({ compareMode: "equals", matchedRuleValue: "516" }).latestRuleLabel
    ).toBeNull();

    expect(
      summarize({
        reasoningTokens: "invalid",
        guardRetryPhase: 1,
        actionTaken: "",
        action: "fallback",
        guardExhaustedAction: false,
        backoffMs: "invalid",
        guardBudgetRemaining: null,
        guardBudgetTotal: "invalid",
      })
    ).toMatchObject({
      latestReasoningTokens: null,
      latestPhase: null,
      latestActionTaken: "fallback",
      latestExhaustedAction: null,
      latestDelayMs: null,
      latestBudgetRemaining: null,
      latestBudgetTotal: null,
    });
    expect(resolveCodexReasoningGuardSummary(null).count).toBe(0);
  });
});
