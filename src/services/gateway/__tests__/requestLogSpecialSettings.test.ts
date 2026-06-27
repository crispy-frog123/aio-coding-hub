import { describe, expect, it } from "vitest";
import {
  countCodexReasoningGuardSpecialSettings,
  hasClaudeModelMappingSpecialSetting,
  resolveCodexReasoningGuardSummary,
  resolveClaudeModelMappingFromSpecialSettings,
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

  it("counts Codex reasoning guard special settings", () => {
    expect(
      countCodexReasoningGuardSpecialSettings(
        JSON.stringify([
          {
            type: "codex_reasoning_guard",
            compareMode: "equals",
            compareModeSymbol: "==",
            matchedRuleValue: 516,
            reasoningTokens: 516,
          },
          { type: "noop" },
          {
            type: "codex_reasoning_guard",
            compareMode: "less_than_or_equal",
            compareModeSymbol: "<=",
            matchedRuleValue: 516,
            reasoningTokens: 300,
          },
        ])
      )
    ).toBe(2);
    expect(
      countCodexReasoningGuardSpecialSettings(JSON.stringify({ type: "codex_reasoning_guard" }))
    ).toBe(1);
    expect(countCodexReasoningGuardSpecialSettings("bad-json")).toBe(0);
  });

  it("resolves Codex reasoning guard summary with latest rule label", () => {
    expect(
      resolveCodexReasoningGuardSummary(
        JSON.stringify([
          {
            type: "codex_reasoning_guard",
            compareMode: "equals",
            matchedRuleValue: 516,
            reasoningTokens: 516,
          },
          {
            type: "codex_reasoning_guard",
            compareMode: "less_than_or_equal",
            compareModeSymbol: "<=",
            matchedRuleValue: 516,
            reasoningTokens: 300,
          },
        ])
      )
    ).toEqual({
      count: 2,
      latestRuleLabel: "<= 516",
      latestReasoningTokens: 300,
    });
  });
});
