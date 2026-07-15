import { normalizeClaudeModelMapping, type ClaudeModelMapping } from "./claudeModelMapping";

export type ParsedRequestLogSpecialSetting = {
  type?: string;
  reason?: string;
} & Record<string, unknown>;

export type CodexReasoningGuardSummary = {
  count: number;
  latestRuleLabel: string | null;
  latestReasoningTokens: number | null;
  latestPhase: string | null;
  latestActionTaken: string | null;
  latestExhaustedAction: string | null;
  latestDelayMs: number | null;
  latestBudgetRemaining: number | null;
  latestBudgetTotal: number | null;
};

export type CodexReasoningGuardCheckSummary = {
  count: number;
  checkedCount: number;
  matchedCount: number;
  latestRuleLabel: string | null;
  latestReasoningTokens: number | null;
  latestReasoningEffort: string | null;
  latestMissReason: string | null;
  latestMissReasonLabel: string | null;
  latestExemptReason: string | null;
};

export type CodexGatewayPolicyAttempt = {
  attemptSequence: number | null;
  providerId: number | null;
  providerName: string | null;
  retryAttemptNumber: number | null;
  policyTrigger: string | null;
  policyAction: string | null;
  retryTrigger: string | null;
  retryDelayMs: number | null;
  retryAfterRaw: string | null;
  retryAfterMs: number | null;
  retryBudgetUsed: number | null;
  retryBudgetRemaining: number | null;
  upstreamFetchStartedAtMs: number | null;
  upstreamHttpStatus: number | null;
  firstProgressAtMs: number | null;
  timeToFirstProgressMs: number | null;
  clientHeadersSentAtMs: number | null;
  clientFirstWriteAtMs: number | null;
  timeToClientFirstWriteMs: number | null;
  timeoutPhase: string | null;
  timeoutLimitMs: number | null;
  timeoutResponseControlLost: boolean;
  responseForwardingStarted: boolean;
  finalAction: string | null;
};

export type CodexGatewayPolicyAttemptSummary = {
  count: number;
  attempts: CodexGatewayPolicyAttempt[];
  latest: CodexGatewayPolicyAttempt | null;
};

export type CodexReasoningEffort =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max"
  | "ultra"
  | "unknown";

export type CodexReasoningEffortSource = "request" | "default" | "unknown";

export type CodexReasoningEffortResolution = {
  effort: CodexReasoningEffort;
  source: CodexReasoningEffortSource;
};

const CODEX_REASONING_EFFORTS = new Set<CodexReasoningEffort>([
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
]);

const KNOWN_CODEX_MODEL_DEFAULT_REASONING_EFFORTS: Readonly<Record<string, CodexReasoningEffort>> =
  {
    "gpt-5.5": "medium",
    "gpt-5.5-pro": "high",
    "gpt-5.4": "none",
    "gpt-5.4-mini": "none",
    "gpt-5.4-nano": "none",
    "gpt-5.4-pro": "medium",
  };

const CODEX_REASONING_EFFORT_FIELD_NAMES = new Set(["effort", "rawEffort"]);

export const CODEX_SYSTEM_REQUEST_SPECIAL_SETTING = {
  type: "codex_system_request",
  threadSource: "system",
} as const;

export function parseRequestLogSpecialSettings(
  specialSettingsJson: string | null | undefined
): ParsedRequestLogSpecialSetting[] {
  if (!specialSettingsJson) return [];

  try {
    const parsed = JSON.parse(specialSettingsJson) as unknown;
    if (Array.isArray(parsed)) {
      return parsed.filter(isParsedRequestLogSpecialSetting);
    }
    return isParsedRequestLogSpecialSetting(parsed) ? [parsed] : [];
  } catch {
    return [];
  }
}

function isParsedRequestLogSpecialSetting(value: unknown): value is ParsedRequestLogSpecialSetting {
  return typeof value === "object" && value !== null;
}

function parsedSettingString(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function parsedSettingNumber(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : Number.NaN;
}

function parsedSettingBoolean(value: unknown): boolean {
  return typeof value === "boolean" ? value : false;
}

function parsedOptionalSettingString(value: unknown): string | null {
  const parsed = parsedSettingString(value);
  return parsed ? parsed : null;
}

function parsedOptionalSettingNumber(value: unknown): number | null {
  const parsed = parsedSettingNumber(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function normalizeCodexReasoningEffort(
  value: unknown
): Exclude<CodexReasoningEffort, "unknown"> | null {
  const effort = parsedSettingString(value).trim().toLowerCase();
  return CODEX_REASONING_EFFORTS.has(effort as CodexReasoningEffort)
    ? (effort as Exclude<CodexReasoningEffort, "unknown">)
    : null;
}

function normalizeRequestedModel(value: string | null | undefined): string | null {
  const model = value?.trim().toLowerCase();
  return model ? model : null;
}

export function resolveCodexReasoningEffort(
  requestedModel: string | null | undefined,
  specialSettingsJson: string | null | undefined
): CodexReasoningEffortResolution {
  const settings = parseRequestLogSpecialSettings(specialSettingsJson);
  const explicitSetting = settings
    .slice()
    .reverse()
    .find((setting) => setting.type === "codex_reasoning_effort");
  const explicitEffort = explicitSetting
    ? normalizeCodexReasoningEffort(explicitSetting.effort)
    : null;

  if (explicitEffort) {
    return { effort: explicitEffort, source: "request" };
  }

  if (explicitSetting && hasCodexReasoningEffortField(explicitSetting)) {
    return { effort: "unknown", source: "unknown" };
  }

  const model = normalizeRequestedModel(requestedModel);
  if (model && KNOWN_CODEX_MODEL_DEFAULT_REASONING_EFFORTS[model]) {
    return {
      effort: KNOWN_CODEX_MODEL_DEFAULT_REASONING_EFFORTS[model],
      source: "default",
    };
  }

  return { effort: "unknown", source: "unknown" };
}

function hasCodexReasoningEffortField(setting: ParsedRequestLogSpecialSetting): boolean {
  return Object.keys(setting).some((key) => CODEX_REASONING_EFFORT_FIELD_NAMES.has(key));
}

export function formatCodexReasoningEffortSource(source: CodexReasoningEffortSource): string {
  if (source === "request") return "请求显式";
  if (source === "default") return "默认推断";
  return "未知";
}

export function resolveClaudeModelMappingFromSpecialSettings(
  specialSettingsJson: string | null | undefined,
  finalProviderId?: number | null
): ClaudeModelMapping | null {
  const settings = parseRequestLogSpecialSettings(specialSettingsJson);
  const mappings = settings
    .map((setting) => {
      if (setting.type !== "claude_model_mapping") return null;
      return normalizeClaudeModelMapping({
        requestedModel: parsedSettingString(setting.requestedModel),
        effectiveModel: parsedSettingString(setting.effectiveModel),
        mappingKind: parsedSettingString(setting.mappingKind),
        providerId: parsedSettingNumber(setting.providerId),
        providerName: parsedSettingString(setting.providerName),
        applied: parsedSettingBoolean(setting.applied),
      });
    })
    .filter((mapping): mapping is ClaudeModelMapping => mapping !== null);

  if (mappings.length === 0) return null;

  if (finalProviderId != null) {
    const finalProviderMapping = mappings
      .slice()
      .reverse()
      .find((mapping) => mapping.providerId === finalProviderId);
    if (finalProviderMapping) return finalProviderMapping;
  }

  return mappings[mappings.length - 1] ?? null;
}

export function hasClaudeModelMappingSpecialSetting(
  specialSettingsJson: string | null | undefined
): boolean {
  const settings = parseRequestLogSpecialSettings(specialSettingsJson);
  for (const setting of settings) {
    if (setting.type !== "claude_model_mapping") continue;
    return true;
  }
  return false;
}

export function countCodexReasoningGuardSpecialSettings(
  specialSettingsJson: string | null | undefined
): number {
  return resolveCodexReasoningGuardSummary(specialSettingsJson).count;
}

function normalizeCodexReasoningGuardCompareSymbol(
  compareMode: unknown,
  compareModeSymbol: unknown
): string | null {
  const explicitSymbol = parsedSettingString(compareModeSymbol);
  if (explicitSymbol === "==" || explicitSymbol === "<=") {
    return explicitSymbol;
  }

  const mode = parsedSettingString(compareMode);
  if (mode === "equals") return "==";
  if (mode === "less_than_or_equal") return "<=";
  return null;
}

function normalizeCodexReasoningGuardRuleMode(value: unknown): string | null {
  const mode = parsedSettingString(value);
  if (mode === "final_answer_only_high_xhigh") return mode;
  if (mode === "reasoning_tokens") return mode;
  return null;
}

function formatCodexReasoningGuardMissReason(reason: string): string {
  const labels: Readonly<Record<string, string>> = {
    context_compaction_exempt: "上下文压缩请求已豁免",
    missing_reasoning_tokens: "响应未提供 reasoning_tokens",
    reasoning_tokens_not_formula_match: "reasoning_tokens 不符合 518n-2",
    no_configured_reasoning_rule: "没有可用的 reasoning_tokens 规则",
    reasoning_tokens_not_matched: "reasoning_tokens 未命中当前规则",
    zero_reasoning_tokens: "reasoning_tokens 为 0",
    reasoning_effort_not_high_xhigh: "思考等级不是 high/xhigh",
    missing_final_answer: "响应中未观察到 final answer",
    commentary_observed: "响应包含 commentary",
    tool_call_observed: "响应包含 tool call",
    reasoning_item_observed: "响应包含 reasoning item",
  };
  return labels[reason] ?? reason;
}

export function resolveCodexReasoningGuardCheckSummary(
  specialSettingsJson: string | null | undefined
): CodexReasoningGuardCheckSummary {
  const settings = parseRequestLogSpecialSettings(specialSettingsJson);
  let count = 0;
  let checkedCount = 0;
  let matchedCount = 0;
  let latestRuleLabel: string | null = null;
  let latestReasoningTokens: number | null = null;
  let latestReasoningEffort: string | null = null;
  let latestMissReason: string | null = null;
  let latestMissReasonLabel: string | null = null;
  let latestExemptReason: string | null = null;

  for (const setting of settings) {
    if (setting.type !== "codex_reasoning_guard_check") continue;
    count += 1;
    if (setting.checked === true) checkedCount += 1;
    if (setting.matched === true) matchedCount += 1;

    const ruleMode = normalizeCodexReasoningGuardRuleMode(setting.ruleMode);
    const matchMode = parsedSettingString(setting.reasoningMatchMode);
    latestRuleLabel =
      ruleMode === "final_answer_only_high_xhigh"
        ? "final-answer-only / high,xhigh"
        : matchMode === "formula_518n_minus_2"
          ? "reasoning_tokens / 518n-2"
          : ruleMode === "reasoning_tokens"
            ? "reasoning_tokens"
            : null;
    const reasoningTokens = parsedSettingNumber(setting.reasoningTokens);
    latestReasoningTokens = Number.isFinite(reasoningTokens) ? reasoningTokens : null;
    latestReasoningEffort = parsedSettingString(setting.reasoningEffort) || null;
    latestMissReason = parsedSettingString(setting.missReason) || null;
    latestMissReasonLabel = latestMissReason
      ? formatCodexReasoningGuardMissReason(latestMissReason)
      : null;
    latestExemptReason = parsedSettingString(setting.interceptExemptReason) || null;
  }

  return {
    count,
    checkedCount,
    matchedCount,
    latestRuleLabel,
    latestReasoningTokens,
    latestReasoningEffort,
    latestMissReason,
    latestMissReasonLabel,
    latestExemptReason,
  };
}

export function resolveCodexReasoningGuardSummary(
  specialSettingsJson: string | null | undefined
): CodexReasoningGuardSummary {
  const settings = parseRequestLogSpecialSettings(specialSettingsJson);
  let count = 0;
  let latestRuleLabel: string | null = null;
  let latestReasoningTokens: number | null = null;
  let latestPhase: string | null = null;
  let latestActionTaken: string | null = null;
  let latestExhaustedAction: string | null = null;
  let latestDelayMs: number | null = null;
  let latestBudgetRemaining: number | null = null;
  let latestBudgetTotal: number | null = null;

  for (const setting of settings) {
    if (setting.type === "codex_reasoning_guard") {
      count += 1;
      const ruleMode = normalizeCodexReasoningGuardRuleMode(setting.ruleMode);
      const compareSymbol = normalizeCodexReasoningGuardCompareSymbol(
        setting.compareMode,
        setting.compareModeSymbol
      );
      const matchedRuleValue = parsedSettingNumber(setting.matchedRuleValue);
      const reasoningTokens = parsedSettingNumber(setting.reasoningTokens);
      latestRuleLabel =
        ruleMode === "final_answer_only_high_xhigh"
          ? "final-answer-only / high,xhigh"
          : compareSymbol && Number.isFinite(matchedRuleValue)
            ? `${compareSymbol} ${matchedRuleValue}`
            : null;
      latestReasoningTokens = Number.isFinite(reasoningTokens) ? reasoningTokens : null;
      latestPhase = parsedSettingString(setting.guardRetryPhase) || null;
      latestActionTaken =
        parsedSettingString(setting.actionTaken) || parsedSettingString(setting.action) || null;
      latestExhaustedAction = parsedSettingString(setting.guardExhaustedAction) || null;
      const delayMs = parsedSettingNumber(setting.backoffMs);
      const budgetRemaining = parsedSettingNumber(setting.guardBudgetRemaining);
      const budgetTotal = parsedSettingNumber(setting.guardBudgetTotal);
      latestDelayMs = Number.isFinite(delayMs) ? delayMs : null;
      latestBudgetRemaining = Number.isFinite(budgetRemaining) ? budgetRemaining : null;
      latestBudgetTotal = Number.isFinite(budgetTotal) ? budgetTotal : null;
    }
  }

  return {
    count,
    latestRuleLabel,
    latestReasoningTokens,
    latestPhase,
    latestActionTaken,
    latestExhaustedAction,
    latestDelayMs,
    latestBudgetRemaining,
    latestBudgetTotal,
  };
}

export function resolveCodexGatewayPolicyAttemptSummary(
  specialSettingsJson: string | null | undefined
): CodexGatewayPolicyAttemptSummary {
  const attempts = parseRequestLogSpecialSettings(specialSettingsJson)
    .filter((setting) => setting.type === "codex_gateway_policy_attempt")
    .map<CodexGatewayPolicyAttempt>((setting) => ({
      attemptSequence: parsedOptionalSettingNumber(setting.attemptSequence),
      providerId: parsedOptionalSettingNumber(setting.providerId),
      providerName: parsedOptionalSettingString(setting.providerName),
      retryAttemptNumber: parsedOptionalSettingNumber(setting.retryAttemptNumber),
      policyTrigger: parsedOptionalSettingString(setting.policyTrigger),
      policyAction: parsedOptionalSettingString(setting.policyAction),
      retryTrigger: parsedOptionalSettingString(setting.retryTrigger),
      retryDelayMs: parsedOptionalSettingNumber(setting.retryDelayMs),
      retryAfterRaw: parsedOptionalSettingString(setting.retryAfterRaw),
      retryAfterMs: parsedOptionalSettingNumber(setting.retryAfterMs),
      retryBudgetUsed: parsedOptionalSettingNumber(setting.retryBudgetUsed),
      retryBudgetRemaining: parsedOptionalSettingNumber(setting.retryBudgetRemaining),
      upstreamFetchStartedAtMs: parsedOptionalSettingNumber(setting.upstreamFetchStartedAtMs),
      upstreamHttpStatus: parsedOptionalSettingNumber(setting.upstreamHttpStatus),
      firstProgressAtMs: parsedOptionalSettingNumber(setting.firstProgressAtMs),
      timeToFirstProgressMs: parsedOptionalSettingNumber(setting.timeToFirstProgressMs),
      clientHeadersSentAtMs: parsedOptionalSettingNumber(setting.clientHeadersSentAtMs),
      clientFirstWriteAtMs: parsedOptionalSettingNumber(setting.clientFirstWriteAtMs),
      timeToClientFirstWriteMs: parsedOptionalSettingNumber(setting.timeToClientFirstWriteMs),
      timeoutPhase: parsedOptionalSettingString(setting.timeoutPhase),
      timeoutLimitMs: parsedOptionalSettingNumber(setting.timeoutLimitMs),
      timeoutResponseControlLost: parsedSettingBoolean(setting.timeoutResponseControlLost),
      responseForwardingStarted: parsedSettingBoolean(setting.responseForwardingStarted),
      finalAction: parsedOptionalSettingString(setting.finalAction),
    }));

  return {
    count: attempts.length,
    attempts,
    latest: attempts[attempts.length - 1] ?? null,
  };
}

export function hasCodexSystemRequestSpecialSetting(
  specialSettingsJson: string | null | undefined
): boolean {
  return parseRequestLogSpecialSettings(specialSettingsJson).some(
    (setting) =>
      setting.type === CODEX_SYSTEM_REQUEST_SPECIAL_SETTING.type &&
      setting.threadSource === CODEX_SYSTEM_REQUEST_SPECIAL_SETTING.threadSource
  );
}
