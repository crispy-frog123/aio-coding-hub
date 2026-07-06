import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  Activity,
  BarChart3,
  BrainCircuit,
  CalendarDays,
  Download,
  FileSearch,
  RefreshCw,
  ShieldAlert,
  Table2,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { toast } from "sonner";
import { useAppSessionStartedAtMs } from "../app/appSession";
import {
  useCodexReasoningAnalyticsAnalyzeQuery,
  useCodexReasoningAnalyticsBackfillMutation,
  useCodexReasoningAnalyticsExportMutation,
  useCodexReasoningAnalyticsImportJsonMutation,
  useCodexReasoningAnalyticsSnapshotQuery,
} from "../query/codexReasoningAnalytics";
import { useRequestLogsCodexReasoningGuardStatsQuery } from "../query/requestLogs";
import {
  getSettingsReadProtection,
  useSettingsPatchMutation,
  useSettingsQuery,
} from "../query/settings";
import type {
  AppSettings,
  CodexReasoningGuardCompareMode,
  CodexReasoningGuardExhaustedAction,
  CodexReasoningGuardMatchMode,
  CodexReasoningGuardRuleMode,
  CodexReasoningGuardStreamAction,
} from "../services/settings/settings";
import {
  DEFAULT_CODEX_REASONING_GUARD_DELAYED_RETRY_BUDGET,
  DEFAULT_CODEX_REASONING_GUARD_DELAYED_RETRY_MS,
  DEFAULT_CODEX_REASONING_GUARD_CONTINUATION_MARKER_TEXT,
  DEFAULT_CODEX_REASONING_GUARD_EXHAUSTED_ACTION,
  DEFAULT_CODEX_REASONING_GUARD_IMMEDIATE_RETRY_BUDGET,
  DEFAULT_CODEX_REASONING_GUARD_REASONING_EQUALS,
  MAX_CODEX_REASONING_GUARD_BACKOFF_AFTER_HITS,
  MAX_CODEX_REASONING_GUARD_BACKOFF_MS,
  MAX_CODEX_REASONING_GUARD_DELAYED_RETRY_BUDGET,
  MAX_CODEX_REASONING_GUARD_DELAYED_RETRY_MS,
  MAX_CODEX_REASONING_GUARD_IMMEDIATE_RETRY_BUDGET,
  MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE,
} from "../services/settings/settingsValidation";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Input } from "../ui/Input";
import { PageHeader } from "../ui/PageHeader";
import { Select } from "../ui/Select";
import { Spinner } from "../ui/Spinner";
import { Switch } from "../ui/Switch";
import { cn } from "../utils/cn";

type StatsWindow = "session" | "all";
type ReasoningGuardPageTab = "rules" | "analytics";

const ANALYTICS_REFETCH_INTERVAL_MS = 10_000;
const ANALYTICS_RECENT_LIMIT = 50;
const DATE_TIME_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
});

type DraftState = {
  enabled: boolean;
  ruleMode: CodexReasoningGuardRuleMode;
  matchMode: CodexReasoningGuardMatchMode;
  streamAction: CodexReasoningGuardStreamAction;
  continuationMarkerText: string;
  compareMode: CodexReasoningGuardCompareMode;
  valuesText: string;
  immediateBudgetText: string;
  delayedBudgetText: string;
  delayedMsText: string;
  exhaustedAction: CodexReasoningGuardExhaustedAction;
  backoffAfterHitsText: string;
  backoffMsText: string;
};

const PERCENT_FORMATTER = new Intl.NumberFormat("zh-CN", {
  style: "percent",
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
});

function formatValues(values: number[] | null | undefined) {
  const resolved =
    values && values.length > 0 ? values : DEFAULT_CODEX_REASONING_GUARD_REASONING_EQUALS;
  return resolved.join(", ");
}

function formatRuleMode(value: CodexReasoningGuardRuleMode) {
  return value === "final_answer_only_high_xhigh"
    ? "final-answer-only / high,xhigh"
    : "reasoning_tokens";
}

function formatCompareMode(value: CodexReasoningGuardCompareMode) {
  return value === "less_than_or_equal" ? "<= 任一值" : "== 任一值";
}

function formatMatchMode(value: CodexReasoningGuardMatchMode) {
  return value === "formula_518n_minus_2" ? "518*n - 2 公式" : "手动 token 列表";
}

function formatStreamAction(value: CodexReasoningGuardStreamAction) {
  if (value === "continuation_recovery") return "续写恢复";
  if (value === "disconnect") return "兼容断开";
  return "标准保护";
}

function formatExhaustedAction(value: CodexReasoningGuardExhaustedAction) {
  return value === "switch_provider" ? "切换 provider" : "返回错误";
}

function formatRate(value: number | null | undefined) {
  return PERCENT_FORMATTER.format(value ?? 0);
}

function formatDateTime(valueMs: number) {
  return DATE_TIME_FORMATTER.format(new Date(valueMs));
}

function downloadTextFile(filename: string, mimeType: string, text: string) {
  const blob = new Blob([text], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function createDraft(settings: AppSettings | null | undefined): DraftState {
  return {
    enabled: settings?.codex_reasoning_guard_enabled ?? false,
    ruleMode: settings?.codex_reasoning_guard_rule_mode ?? "reasoning_tokens",
    matchMode: settings?.codex_reasoning_guard_match_mode ?? "formula_518n_minus_2",
    streamAction: settings?.codex_reasoning_guard_stream_action ?? "continuation_recovery",
    continuationMarkerText:
      settings?.codex_reasoning_guard_continuation_marker_text ??
      DEFAULT_CODEX_REASONING_GUARD_CONTINUATION_MARKER_TEXT,
    compareMode: settings?.codex_reasoning_guard_compare_mode ?? "equals",
    valuesText: formatValues(settings?.codex_reasoning_guard_reasoning_equals),
    immediateBudgetText: String(
      settings?.codex_reasoning_guard_immediate_retry_budget ??
        DEFAULT_CODEX_REASONING_GUARD_IMMEDIATE_RETRY_BUDGET
    ),
    delayedBudgetText: String(
      settings?.codex_reasoning_guard_delayed_retry_budget ??
        DEFAULT_CODEX_REASONING_GUARD_DELAYED_RETRY_BUDGET
    ),
    delayedMsText: String(
      settings?.codex_reasoning_guard_delayed_retry_ms ??
        DEFAULT_CODEX_REASONING_GUARD_DELAYED_RETRY_MS
    ),
    exhaustedAction:
      settings?.codex_reasoning_guard_exhausted_action ??
      DEFAULT_CODEX_REASONING_GUARD_EXHAUSTED_ACTION,
    backoffAfterHitsText: String(settings?.codex_reasoning_guard_backoff_after_hits ?? 5),
    backoffMsText: String(settings?.codex_reasoning_guard_backoff_ms ?? 1000),
  };
}

function parseValues(raw: string): { ok: true; values: number[] } | { ok: false; message: string } {
  const parts = raw
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);

  if (parts.length === 0) {
    return { ok: false, message: "至少填写一个 reasoning_tokens 值" };
  }

  const values = parts.map((part) => Number(part));
  if (values.some((value) => !Number.isSafeInteger(value))) {
    return { ok: false, message: "reasoning_tokens 必须是整数，多个值用英文逗号分隔" };
  }
  if (
    values.some((value) => value < 0 || value > MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE)
  ) {
    return {
      ok: false,
      message: `reasoning_tokens 必须在 0 到 ${MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE} 之间`,
    };
  }

  return { ok: true, values };
}

function parseInteger(
  label: string,
  raw: string,
  max: number
): { ok: true; value: number } | { ok: false; message: string } {
  const value = Number(raw.trim());
  if (!Number.isSafeInteger(value) || value < 0 || value > max) {
    return { ok: false, message: `${label}必须是 0-${max} 的整数` };
  }
  return { ok: true, value };
}

function MetricTile({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-lg border border-line-subtle bg-surface-inset px-3 py-2.5">
      <div className="text-[11px] font-medium text-muted-foreground">{label}</div>
      <div className="mt-1 font-mono text-lg font-semibold text-foreground">{value}</div>
      {hint ? <div className="mt-1 text-[11px] text-muted-foreground">{hint}</div> : null}
    </div>
  );
}

function FieldLabel({ label, hint }: { label: string; hint?: string }) {
  return (
    <label className="space-y-1.5">
      <span className="block text-xs font-semibold text-muted-foreground">{label}</span>
      {hint ? <span className="block text-[11px] text-muted-foreground">{hint}</span> : null}
    </label>
  );
}

function PageTabButton({
  active,
  label,
  description,
  onClick,
}: {
  active: boolean;
  label: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "min-w-[180px] rounded-lg border px-3 py-2 text-left transition",
        active
          ? "border-state-selected-border bg-state-selected text-state-selected-foreground"
          : "border-line bg-surface-panel text-foreground hover:bg-state-hover"
      )}
    >
      <span className="block text-sm font-semibold">{label}</span>
      <span
        className={cn("mt-0.5 block text-[11px]", active ? "opacity-80" : "text-muted-foreground")}
      >
        {description}
      </span>
    </button>
  );
}

function AnalyticsDataCard({
  icon: Icon,
  title,
  badge,
  children,
}: {
  icon: LucideIcon;
  title: string;
  badge?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex h-80 flex-col rounded-lg border border-line-subtle bg-surface-inset px-3 py-3">
      <div className="flex shrink-0 items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
          <span className="truncate text-sm font-semibold text-foreground">{title}</span>
        </div>
        {badge ? (
          <Badge variant="outline" className="shrink-0 text-[10px]">
            {badge}
          </Badge>
        ) : null}
      </div>
      <div className="mt-3 min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
    </div>
  );
}

function MiniRows({
  rows,
  emptyText,
}: {
  rows: Array<{ label: string; value: string; hint?: string }>;
  emptyText: string;
}) {
  if (rows.length === 0) {
    return (
      <div className="rounded-md border border-dashed border-line-subtle px-3 py-4 text-xs text-muted-foreground">
        {emptyText}
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      {rows.map((row) => (
        <div
          key={`${row.label}-${row.hint ?? ""}`}
          className="flex items-center justify-between gap-3 rounded-md bg-surface-panel px-2.5 py-2"
        >
          <div className="min-w-0">
            <div className="truncate font-mono text-xs font-semibold text-foreground">
              {row.label}
            </div>
            {row.hint ? (
              <div className="mt-0.5 truncate text-[11px] text-muted-foreground">{row.hint}</div>
            ) : null}
          </div>
          <div className="shrink-0 font-mono text-xs font-semibold text-muted-foreground">
            {row.value}
          </div>
        </div>
      ))}
    </div>
  );
}

export function ReasoningGuardPage() {
  const settingsQuery = useSettingsQuery();
  const settings = settingsQuery.data ?? null;
  const settingsPatchMutation = useSettingsPatchMutation();
  const appSessionStartedAtMs = useAppSessionStartedAtMs();
  const [activePageTab, setActivePageTab] = useState<ReasoningGuardPageTab>("rules");
  const [statsWindow, setStatsWindow] = useState<StatsWindow>("session");
  const [draft, setDraft] = useState<DraftState>(() => createDraft(null));
  const [formError, setFormError] = useState<string | null>(null);
  const [importJsonText, setImportJsonText] = useState("");

  const analyticsPollingMs = activePageTab === "analytics" ? ANALYTICS_REFETCH_INTERVAL_MS : false;
  const sessionStatsQuery = useRequestLogsCodexReasoningGuardStatsQuery(appSessionStartedAtMs, {
    refetchIntervalMs: analyticsPollingMs,
  });
  const allStatsQuery = useRequestLogsCodexReasoningGuardStatsQuery(null, {
    refetchIntervalMs: analyticsPollingMs,
  });
  const analyticsSnapshotQuery = useCodexReasoningAnalyticsSnapshotQuery(
    {
      dateFrom: null,
      dateTo: null,
      recentLimit: ANALYTICS_RECENT_LIMIT,
    },
    {
      enabled: activePageTab === "analytics",
      refetchIntervalMs: analyticsPollingMs,
    }
  );
  const analyticsAnalyzeQuery = useCodexReasoningAnalyticsAnalyzeQuery(
    {
      dateFrom: null,
      dateTo: null,
      reasoningTokens: [516],
    },
    {
      enabled: activePageTab === "analytics",
      refetchIntervalMs: analyticsPollingMs,
    }
  );
  const analyticsBackfillMutation = useCodexReasoningAnalyticsBackfillMutation();
  const analyticsImportMutation = useCodexReasoningAnalyticsImportJsonMutation();
  const analyticsExportMutation = useCodexReasoningAnalyticsExportMutation();
  const statsQuery = statsWindow === "session" ? sessionStatsQuery : allStatsQuery;
  const stats = statsQuery.data ?? null;
  const { settingsReadErrorMessage, settingsWriteBlocked } =
    getSettingsReadProtection(settingsQuery);
  const settingsLoading = settingsQuery.isLoading && !settings;
  const settingsSaving = settingsPatchMutation.isPending || settingsWriteBlocked;

  useEffect(() => {
    if (settings) {
      setDraft(createDraft(settings));
      setFormError(null);
    }
  }, [settings]);

  const sortedModelStats = useMemo(() => {
    return [...(stats?.by_model ?? [])].sort((left, right) => {
      if (right.hit_request_count !== left.hit_request_count) {
        return right.hit_request_count - left.hit_request_count;
      }
      return right.total_request_count - left.total_request_count;
    });
  }, [stats?.by_model]);

  const analyticsSnapshot = analyticsSnapshotQuery.data ?? null;
  const analyticsAnalysis = analyticsAnalyzeQuery.data ?? null;
  const topReasoningTokens = analyticsSnapshot?.top_reasoning_tokens ?? [];
  const modelFamilyAndEffortRows = analyticsSnapshot?.by_model_family_and_effort ?? [];
  const candidatePatternRows = analyticsSnapshot?.candidate_patterns ?? [];
  const recentSamples = analyticsSnapshot?.recent_samples ?? [];
  const analyticsSampleCount = analyticsSnapshot?.summary.total_samples ?? 0;

  const guardEnabledLabel = draft.enabled ? "已开启" : "已关闭";
  const usesFinalOnlyMode = draft.ruleMode === "final_answer_only_high_xhigh";
  const usesFormulaMatchMode = draft.matchMode === "formula_518n_minus_2";
  const modelRules = settings?.codex_reasoning_guard_model_rules ?? [];

  async function saveSettings() {
    if (!settings) return;
    if (settingsWriteBlocked) {
      toast(settingsReadErrorMessage ?? "设置文件读取失败，当前处于只读保护");
      return;
    }

    const values = parseValues(draft.valuesText);
    if (!values.ok) {
      setFormError(values.message);
      return;
    }
    const immediateBudget = parseInteger(
      "立即重试预算",
      draft.immediateBudgetText,
      MAX_CODEX_REASONING_GUARD_IMMEDIATE_RETRY_BUDGET
    );
    if (!immediateBudget.ok) {
      setFormError(immediateBudget.message);
      return;
    }
    const delayedBudget = parseInteger(
      "等待重试预算",
      draft.delayedBudgetText,
      MAX_CODEX_REASONING_GUARD_DELAYED_RETRY_BUDGET
    );
    if (!delayedBudget.ok) {
      setFormError(delayedBudget.message);
      return;
    }
    const delayedMs = parseInteger(
      "等待时间",
      draft.delayedMsText,
      MAX_CODEX_REASONING_GUARD_DELAYED_RETRY_MS
    );
    if (!delayedMs.ok) {
      setFormError(delayedMs.message);
      return;
    }
    const backoffAfterHits = parseInteger(
      "退避触发次数",
      draft.backoffAfterHitsText,
      MAX_CODEX_REASONING_GUARD_BACKOFF_AFTER_HITS
    );
    if (!backoffAfterHits.ok) {
      setFormError(backoffAfterHits.message);
      return;
    }
    const backoffMs = parseInteger(
      "退避等待时间",
      draft.backoffMsText,
      MAX_CODEX_REASONING_GUARD_BACKOFF_MS
    );
    if (!backoffMs.ok) {
      setFormError(backoffMs.message);
      return;
    }
    const continuationMarkerText =
      draft.continuationMarkerText.trim() || DEFAULT_CODEX_REASONING_GUARD_CONTINUATION_MARKER_TEXT;

    setFormError(null);
    try {
      await settingsPatchMutation.mutateAsync({
        codex_reasoning_guard_enabled: draft.enabled,
        codex_reasoning_guard_rule_mode: draft.ruleMode,
        codex_reasoning_guard_match_mode: draft.matchMode,
        codex_reasoning_guard_compare_mode: draft.compareMode,
        codex_reasoning_guard_reasoning_equals: values.values,
        codex_reasoning_guard_stream_action: draft.streamAction,
        codex_reasoning_guard_continuation_marker_text: continuationMarkerText,
        codex_reasoning_guard_immediate_retry_budget: immediateBudget.value,
        codex_reasoning_guard_delayed_retry_budget: delayedBudget.value,
        codex_reasoning_guard_delayed_retry_ms: delayedMs.value,
        codex_reasoning_guard_exhausted_action: draft.exhaustedAction,
        codex_reasoning_guard_backoff_after_hits: backoffAfterHits.value,
        codex_reasoning_guard_backoff_ms: backoffMs.value,
      });
      toast("降智拦截设置已保存");
    } catch (err) {
      toast(`保存失败：${String(err)}`);
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-hidden">
      <PageHeader
        title="降智拦截"
        subtitle="Codex gateway 降智拦截规则、重试预算与 reasoning analytics。CLI 管理里的原入口保持不变。"
        actions={
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                void Promise.all([
                  sessionStatsQuery.refetch(),
                  allStatsQuery.refetch(),
                  analyticsSnapshotQuery.refetch(),
                  analyticsAnalyzeQuery.refetch(),
                ]);
              }}
              disabled={
                sessionStatsQuery.isFetching ||
                allStatsQuery.isFetching ||
                analyticsSnapshotQuery.isFetching ||
                analyticsAnalyzeQuery.isFetching
              }
            >
              <RefreshCw
                className={cn(
                  "h-3.5 w-3.5",
                  sessionStatsQuery.isFetching ||
                    allStatsQuery.isFetching ||
                    analyticsSnapshotQuery.isFetching ||
                    analyticsAnalyzeQuery.isFetching
                    ? "animate-spin"
                    : null
                )}
              />
              刷新统计
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={() => void saveSettings()}
              disabled={settingsSaving || !settings}
            >
              保存设置
            </Button>
          </div>
        }
      />

      {settingsLoading ? (
        <Card className="flex min-h-[220px] items-center justify-center">
          <Spinner />
        </Card>
      ) : settingsReadErrorMessage ? (
        <Card>
          <div className="text-sm text-destructive">{settingsReadErrorMessage}</div>
        </Card>
      ) : null}

      <div className="flex flex-wrap gap-2">
        <PageTabButton
          active={activePageTab === "rules"}
          label="规则设置"
          description="Gateway 降智拦截与预算"
          onClick={() => setActivePageTab("rules")}
        />
        <PageTabButton
          active={activePageTab === "analytics"}
          label="Reasoning analytics"
          description="完整分析页与后续数据模块"
          onClick={() => setActivePageTab("analytics")}
        />
      </div>

      <div className="grid grid-cols-1 gap-3 xl:grid-cols-4">
        <MetricTile
          label="状态"
          value={guardEnabledLabel}
          hint={draft.enabled ? "命中后进入 AIO 预算系统" : "仅记录普通请求"}
        />
        <MetricTile
          label="规则模式"
          value={formatRuleMode(draft.ruleMode)}
          hint={
            usesFinalOnlyMode ? "high/xhigh + final answer only" : formatMatchMode(draft.matchMode)
          }
        />
        <MetricTile
          label="流式动作"
          value={formatStreamAction(draft.streamAction)}
          hint={
            draft.streamAction === "continuation_recovery"
              ? "Responses 命中后先续写"
              : formatCompareMode(draft.compareMode)
          }
        />
        <MetricTile
          label="命中请求"
          value={String(stats?.hit_request_count ?? 0)}
          hint={`${statsWindow === "session" ? "本次启动" : "全部"} / 尝试 ${stats?.hit_attempt_count ?? 0}`}
        />
      </div>

      <div className="scrollbar-overlay min-h-0 flex-1 overflow-y-auto pr-1 pb-8">
        {activePageTab === "rules" ? (
          <>
            <div className="grid grid-cols-1 gap-4">
              <Card className="space-y-5">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <div className="flex items-center gap-2">
                      <ShieldAlert className="h-4 w-4 text-primary" />
                      <h2 className="text-base font-semibold text-foreground">Gateway 规则</h2>
                    </div>
                    <p className="mt-1 text-xs text-muted-foreground">
                      这里保存的是 Codex 降智拦截的同一套全局设置。
                    </p>
                  </div>
                  <div className="flex items-center gap-2 rounded-lg border border-line-subtle bg-surface-inset px-3 py-2">
                    <span className="text-xs font-medium text-muted-foreground">启用</span>
                    <Switch
                      checked={draft.enabled}
                      onCheckedChange={(enabled) => setDraft((prev) => ({ ...prev, enabled }))}
                      disabled={!settings}
                      size="sm"
                    />
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                  <div className="space-y-1.5">
                    <FieldLabel label="规则模式" />
                    <div className="grid grid-cols-2 gap-2">
                      {(
                        [
                          ["reasoning_tokens", "Tokens"],
                          ["final_answer_only_high_xhigh", "Final-only"],
                        ] as const
                      ).map(([value, label]) => (
                        <button
                          key={value}
                          type="button"
                          onClick={() => setDraft((prev) => ({ ...prev, ruleMode: value }))}
                          className={cn(
                            "rounded-lg border px-3 py-2 text-sm font-semibold transition",
                            draft.ruleMode === value
                              ? "border-state-selected-border bg-state-selected text-state-selected-foreground"
                              : "border-line bg-surface-panel text-foreground hover:bg-state-hover"
                          )}
                        >
                          {label}
                        </button>
                      ))}
                    </div>
                    <p className="text-[11px] text-muted-foreground">
                      Final-only 模式只拦截显式 high/xhigh 且响应为纯最终答案的请求。
                    </p>
                  </div>

                  <div
                    className={cn("space-y-1.5", usesFinalOnlyMode ? "opacity-60" : null)}
                    aria-disabled={usesFinalOnlyMode}
                  >
                    <FieldLabel label="命中条件来源" />
                    <Select
                      value={draft.matchMode}
                      disabled={usesFinalOnlyMode}
                      onChange={(event) => {
                        const value = event.currentTarget.value as CodexReasoningGuardMatchMode;
                        setDraft((prev) => ({ ...prev, matchMode: value }));
                      }}
                    >
                      <option value="formula_518n_minus_2">518*n - 2 公式</option>
                      <option value="manual">手动 reasoning_tokens 列表</option>
                    </Select>
                    <p className="text-[11px] text-muted-foreground">
                      {usesFinalOnlyMode
                        ? "Final-only 模式不会使用 token 命中来源。"
                        : "公式模式匹配 516 / 1034 / 1552 / 2070...；手动模式使用下面的 token 列表和模型规则。"}
                    </p>
                  </div>
                </div>

                <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
                  <div
                    className={cn(
                      "space-y-1.5",
                      usesFinalOnlyMode || usesFormulaMatchMode ? "opacity-60" : null
                    )}
                    aria-disabled={usesFinalOnlyMode || usesFormulaMatchMode}
                  >
                    <FieldLabel label="比较模式" />
                    <Select
                      value={draft.compareMode}
                      disabled={usesFinalOnlyMode || usesFormulaMatchMode}
                      onChange={(event) => {
                        const value = event.currentTarget.value as CodexReasoningGuardCompareMode;
                        setDraft((prev) => ({
                          ...prev,
                          compareMode: value,
                        }));
                      }}
                    >
                      <option value="equals">等于任一 token 值</option>
                      <option value="less_than_or_equal">小于等于任一 token 值</option>
                    </Select>
                    <p className="text-[11px] text-muted-foreground">
                      {usesFinalOnlyMode
                        ? "当前模式不会使用 token 比较规则，但会保留配置，切回 Tokens 后继续生效。"
                        : usesFormulaMatchMode
                          ? "公式模式不使用比较模式；切回手动列表后继续生效。"
                          : "用于判断 reasoning_tokens 是否命中全局或模型级 token 规则。"}
                    </p>
                  </div>

                  <div className="space-y-1.5">
                    <FieldLabel label="流式命中动作" />
                    <Select
                      value={draft.streamAction}
                      onChange={(event) => {
                        const value = event.currentTarget.value as CodexReasoningGuardStreamAction;
                        setDraft((prev) => ({ ...prev, streamAction: value }));
                      }}
                    >
                      <option value="continuation_recovery">续写恢复：Responses 流式先续写</option>
                      <option value="strict_502">标准保护：预算耗尽后返回错误</option>
                      <option value="disconnect">兼容断开：按标准保护处理</option>
                    </Select>
                    <p className="text-[11px] text-muted-foreground">
                      续写恢复不是新规则，只改变流式 Responses 命中后的处理动作；预算仍使用下面的
                      AIO 预算。
                    </p>
                  </div>
                </div>

                <div
                  className={cn(
                    "space-y-1.5",
                    usesFinalOnlyMode || usesFormulaMatchMode ? "opacity-60" : null
                  )}
                  aria-disabled={usesFinalOnlyMode || usesFormulaMatchMode}
                >
                  <FieldLabel
                    label="全局 reasoning_tokens"
                    hint={
                      usesFinalOnlyMode
                        ? "当前模式不会使用 token 规则，但会保留配置，切回 Tokens 后继续生效。"
                        : usesFormulaMatchMode
                          ? "公式模式会自动匹配 518*n - 2；这里的列表只在手动模式生效。"
                          : "多个值用英文逗号分隔。"
                    }
                  />
                  <Input
                    value={draft.valuesText}
                    disabled={usesFinalOnlyMode || usesFormulaMatchMode}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      setDraft((prev) => ({ ...prev, valuesText: value }));
                      setFormError(null);
                    }}
                    mono
                    placeholder="516, 1034, 1552"
                  />
                </div>

                <div className="space-y-1.5">
                  <FieldLabel
                    label="续写 marker"
                    hint="stream_action=continuation_recovery 时追加到下一轮 Responses input。"
                  />
                  <Input
                    value={draft.continuationMarkerText}
                    onChange={(event) => {
                      const value = event.currentTarget.value;
                      setDraft((prev) => ({ ...prev, continuationMarkerText: value }));
                      setFormError(null);
                    }}
                    placeholder={DEFAULT_CODEX_REASONING_GUARD_CONTINUATION_MARKER_TEXT}
                  />
                </div>

                <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
                  <div className="space-y-1.5">
                    <FieldLabel label="立即预算" />
                    <Input
                      value={draft.immediateBudgetText}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        setDraft((prev) => ({ ...prev, immediateBudgetText: value }));
                      }}
                      mono
                    />
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel label="等待预算" />
                    <Input
                      value={draft.delayedBudgetText}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        setDraft((prev) => ({ ...prev, delayedBudgetText: value }));
                      }}
                      mono
                    />
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel label="等待 ms" />
                    <Input
                      value={draft.delayedMsText}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        setDraft((prev) => ({ ...prev, delayedMsText: value }));
                      }}
                      mono
                    />
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel label="耗尽动作" />
                    <Select
                      value={draft.exhaustedAction}
                      onChange={(event) => {
                        const value = event.currentTarget
                          .value as CodexReasoningGuardExhaustedAction;
                        setDraft((prev) => ({
                          ...prev,
                          exhaustedAction: value,
                        }));
                      }}
                    >
                      <option value="return_error">返回错误</option>
                      <option value="switch_provider">切换 provider</option>
                    </Select>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <FieldLabel label="退避触发次数" hint="连续命中到达该次数后进入等待重试。" />
                    <Input
                      value={draft.backoffAfterHitsText}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        setDraft((prev) => ({ ...prev, backoffAfterHitsText: value }));
                      }}
                      mono
                    />
                  </div>
                  <div className="space-y-1.5">
                    <FieldLabel label="退避等待 ms" hint="进入等待重试后，每次等待的毫秒数。" />
                    <Input
                      value={draft.backoffMsText}
                      onChange={(event) => {
                        const value = event.currentTarget.value;
                        setDraft((prev) => ({ ...prev, backoffMsText: value }));
                      }}
                      mono
                    />
                  </div>
                </div>

                {formError ? (
                  <div className="text-xs font-medium text-destructive">{formError}</div>
                ) : null}

                <div className="flex flex-wrap items-center gap-2 border-t border-line-subtle pt-4">
                  <Badge variant={draft.enabled ? "default" : "outline"}>{guardEnabledLabel}</Badge>
                  <Badge variant="secondary">{formatRuleMode(draft.ruleMode)}</Badge>
                  <Badge variant="outline">{formatExhaustedAction(draft.exhaustedAction)}</Badge>
                  <span className="text-xs text-muted-foreground">
                    context compaction 命中豁免；上游 capacity 错误会写入特殊日志标记。
                  </span>
                </div>
              </Card>
            </div>

            <Card
              className={cn(
                "space-y-3",
                usesFinalOnlyMode || usesFormulaMatchMode ? "opacity-60" : null
              )}
              aria-disabled={usesFinalOnlyMode || usesFormulaMatchMode}
            >
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <h2 className="text-base font-semibold text-foreground">模型级规则</h2>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {usesFinalOnlyMode
                      ? "当前 Final-only 模式不会使用模型级 token 规则；配置会保留，切回 Tokens 后继续生效。"
                      : usesFormulaMatchMode
                        ? "当前公式模式不会使用模型级 token 规则；配置会保留，切回手动列表后继续生效。"
                        : "第一版先展示当前配置；完整新增/编辑仍保留在 CLI 管理的 Codex 降智拦截弹窗中。"}
                  </p>
                </div>
                <Badge variant={modelRules.length > 0 ? "secondary" : "outline"}>
                  {modelRules.length} 条
                </Badge>
              </div>
              {modelRules.length === 0 ? (
                <div className="rounded-lg border border-dashed border-line-subtle px-3 py-4 text-sm text-muted-foreground">
                  当前没有模型级覆盖规则，所有模型使用全局规则。
                </div>
              ) : (
                <div className="overflow-hidden rounded-lg border border-line-subtle">
                  <div className="grid grid-cols-[minmax(0,1fr)_140px_minmax(0,1fr)] gap-2 border-b border-line-subtle bg-surface-inset px-3 py-2 text-[11px] font-semibold text-muted-foreground">
                    <span>模型</span>
                    <span>比较</span>
                    <span>reasoning_tokens</span>
                  </div>
                  <div className="divide-y divide-line-subtle">
                    {modelRules.map((rule) => (
                      <div
                        key={rule.requested_model}
                        className="grid grid-cols-[minmax(0,1fr)_140px_minmax(0,1fr)] gap-2 px-3 py-2 text-xs"
                      >
                        <span className="truncate font-mono text-foreground">
                          {rule.requested_model}
                        </span>
                        <span className="text-muted-foreground">
                          {formatCompareMode(rule.compare_mode ?? "equals")}
                        </span>
                        <span className="truncate font-mono text-muted-foreground">
                          {formatValues(rule.reasoning_equals)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </Card>
          </>
        ) : (
          <div className="space-y-4">
            <Card className="space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <div className="flex items-center gap-2">
                    <BrainCircuit className="h-4 w-4 text-primary" />
                    <h2 className="text-base font-semibold text-foreground">Reasoning analytics</h2>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    当前版本使用后端 SQLite analytics 样本表；analytics 页打开时每 10
                    秒自动刷新，并自动回填最近请求日志。
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="secondary" className="text-[10px]">
                    {analyticsSnapshotQuery.isFetching ? "刷新中" : "10s 轮询"}
                  </Badge>
                  <div className="flex rounded-lg border border-line bg-surface-inset p-0.5">
                    {(
                      [
                        ["session", "本次"],
                        ["all", "最近"],
                      ] as const
                    ).map(([value, label]) => (
                      <button
                        key={value}
                        type="button"
                        onClick={() => setStatsWindow(value)}
                        className={cn(
                          "rounded-md px-2.5 py-1 text-xs font-semibold transition",
                          statsWindow === value
                            ? "bg-surface-panel text-foreground shadow-sm"
                            : "text-muted-foreground hover:text-foreground"
                        )}
                      >
                        {label}
                      </button>
                    ))}
                  </div>
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3 md:grid-cols-3 xl:grid-cols-6">
                <MetricTile label="总请求" value={String(stats?.total_request_count ?? 0)} />
                <MetricTile label="命中请求" value={String(stats?.hit_request_count ?? 0)} />
                <MetricTile label="命中尝试" value={String(stats?.hit_attempt_count ?? 0)} />
                <MetricTile label="正常请求" value={String(stats?.normal_request_count ?? 0)} />
                <MetricTile
                  label="续写恢复"
                  value={String(analyticsSnapshot?.summary.continuation_recovery_count ?? 0)}
                  hint={`成功 ${analyticsSnapshot?.summary.continuation_recovery_success_count ?? 0}`}
                />
                <MetricTile
                  label="续写成功率"
                  value={formatRate(analyticsSnapshot?.summary.continuation_recovery_success_ratio)}
                />
              </div>

              {statsQuery.isFetching && !stats ? (
                <div className="flex min-h-[180px] items-center justify-center">
                  <Spinner />
                </div>
              ) : sortedModelStats.length === 0 ? (
                <EmptyState
                  title="暂无降智拦截统计"
                  description="发起 Codex 请求并触发降智拦截后，这里会显示模型维度的命中情况。"
                />
              ) : (
                <div className="overflow-hidden rounded-lg border border-line-subtle">
                  <div className="grid grid-cols-[minmax(0,1.4fr)_120px_120px_120px_120px] gap-2 border-b border-line-subtle bg-surface-inset px-3 py-2 text-[11px] font-semibold text-muted-foreground">
                    <span>模型</span>
                    <span className="text-right">命中请求</span>
                    <span className="text-right">正常请求</span>
                    <span className="text-right">总请求</span>
                    <span className="text-right">命中率</span>
                  </div>
                  <div className="divide-y divide-line-subtle">
                    {sortedModelStats.map((row) => (
                      <div
                        key={row.requested_model}
                        className="grid grid-cols-[minmax(0,1.4fr)_120px_120px_120px_120px] gap-2 px-3 py-2 text-xs"
                      >
                        <span
                          className="truncate font-mono text-foreground"
                          title={row.requested_model}
                        >
                          {row.requested_model}
                        </span>
                        <span className="text-right font-mono text-foreground">
                          {row.hit_request_count}
                        </span>
                        <span className="text-right font-mono text-muted-foreground">
                          {row.normal_request_count}
                        </span>
                        <span className="text-right font-mono text-muted-foreground">
                          {row.total_request_count}
                        </span>
                        <span className="text-right font-mono text-muted-foreground">
                          {formatRate(row.hit_rate)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </Card>

            <div className="grid grid-cols-1 gap-3 xl:grid-cols-3">
              <AnalyticsDataCard
                icon={BarChart3}
                title="Top reasoning tokens"
                badge={`${topReasoningTokens.length} 项`}
              >
                <MiniRows
                  rows={topReasoningTokens.map((row) => ({
                    label: String(row.value),
                    value: `${row.count} 次`,
                    hint: `占比 ${formatRate(row.ratio)}`,
                  }))}
                  emptyText="后端样本表里还没有带 reasoning_tokens 的样本。"
                />
              </AnalyticsDataCard>

              <AnalyticsDataCard
                icon={Table2}
                title="模型与思考等级"
                badge={`${modelFamilyAndEffortRows.length} 组`}
              >
                <MiniRows
                  rows={modelFamilyAndEffortRows.map((row) => ({
                    label: row.group_label,
                    value: `${row.count} 条`,
                    hint: `final-only ${formatRate(row.final_answer_only_ratio)} / commentary ${formatRate(row.commentary_observed_ratio)}`,
                  }))}
                  emptyText="后端样本表里还没有 Codex 样本。"
                />
              </AnalyticsDataCard>

              <AnalyticsDataCard
                icon={FileSearch}
                title="最近样本与候选特征"
                badge={`${recentSamples.length} 条`}
              >
                <div className="space-y-2">
                  <MiniRows
                    rows={recentSamples.map((sample) => ({
                      label: `${formatDateTime(Date.parse(sample.ts))} · ${sample.request_model ?? "unknown"}`,
                      value:
                        sample.client_http_status == null
                          ? "进行中"
                          : String(sample.client_http_status),
                      hint: `reasoning=${sample.reasoning_tokens ?? "unknown"} | effort=${sample.request_reasoning_effort ?? "unknown"} | final-only=${sample.final_answer_only ? "yes" : "no"} | kind=${sample.request_kind}`,
                    }))}
                    emptyText="后端样本表里还没有 Codex 请求。"
                  />
                  {candidatePatternRows.length > 0 ? (
                    <div className="border-t border-line-subtle pt-2">
                      <div className="mb-1.5 text-[11px] font-semibold text-muted-foreground">
                        候选特征
                      </div>
                      <MiniRows
                        rows={candidatePatternRows.map((row) => ({
                          label: row.pattern_key,
                          value: `${row.count} 条`,
                          hint: row.status,
                        }))}
                        emptyText="暂无候选特征。"
                      />
                    </div>
                  ) : null}
                </div>
              </AnalyticsDataCard>

              <AnalyticsDataCard
                icon={Download}
                title="JSON / CSV 导出"
                badge={`${analyticsSampleCount} 条`}
              >
                <div className="space-y-3">
                  <p className="text-xs leading-5 text-muted-foreground">
                    导出后端 analytics 样本表聚合结果；JSON 为完整 snapshot，CSV 为样本明细。
                  </p>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={async () => {
                        const result = await analyticsExportMutation.mutateAsync({
                          dateFrom: null,
                          dateTo: null,
                          format: "json",
                        });
                        downloadTextFile(
                          result.file_name,
                          "application/json;charset=utf-8",
                          result.content
                        );
                      }}
                      disabled={analyticsSampleCount === 0 || analyticsExportMutation.isPending}
                    >
                      导出 JSON
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={async () => {
                        const result = await analyticsExportMutation.mutateAsync({
                          dateFrom: null,
                          dateTo: null,
                          format: "csv",
                        });
                        downloadTextFile(
                          result.file_name,
                          "text/csv;charset=utf-8",
                          result.content
                        );
                      }}
                      disabled={analyticsSampleCount === 0 || analyticsExportMutation.isPending}
                    >
                      导出 CSV
                    </Button>
                  </div>
                </div>
              </AnalyticsDataCard>

              <AnalyticsDataCard icon={CalendarDays} title="存储与回填" badge="SQLite">
                <div className="space-y-3">
                  <div className="grid grid-cols-2 gap-2">
                    <MetricTile label="存储样本" value={String(analyticsSampleCount)} />
                    <MetricTile
                      label="结构覆盖"
                      value={formatRate(analyticsAnalysis?.field_coverage.final_answer_only)}
                    />
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={async () => {
                      const report = await analyticsBackfillMutation.mutateAsync({
                        sinceCreatedAtMs: null,
                        limit: 10_000,
                      });
                      toast(`已回填 ${report.inserted_or_updated} 条，扫描 ${report.scanned} 条`);
                      void Promise.all([
                        analyticsSnapshotQuery.refetch(),
                        analyticsAnalyzeQuery.refetch(),
                      ]);
                    }}
                    disabled={analyticsBackfillMutation.isPending}
                  >
                    回填最近 10000 条
                  </Button>
                </div>
              </AnalyticsDataCard>

              <AnalyticsDataCard
                icon={Activity}
                title="历史导入与分析 Profile"
                badge={analyticsAnalysis?.analysis_value ?? "暂无样本"}
              >
                <div className="space-y-2">
                  <MiniRows
                    rows={[
                      {
                        label: "reasoning_tokens 覆盖",
                        value: formatRate(analyticsAnalysis?.field_coverage.reasoning_tokens),
                      },
                      {
                        label: "final_answer_only 覆盖",
                        value: formatRate(analyticsAnalysis?.field_coverage.final_answer_only),
                      },
                      {
                        label: "commentary 覆盖",
                        value: formatRate(analyticsAnalysis?.field_coverage.commentary_observed),
                      },
                      {
                        label: "候选样本",
                        value: String(analyticsAnalysis?.candidate_summary.candidate_count ?? 0),
                      },
                      {
                        label: "结论",
                        value: analyticsAnalysis?.conclusion ?? "-",
                      },
                    ]}
                    emptyText="暂无样本可生成 Profile。"
                  />
                  <textarea
                    value={importJsonText}
                    onChange={(event) => setImportJsonText(event.currentTarget.value)}
                    className="min-h-20 w-full resize-y rounded-lg border border-line bg-surface-panel px-3 py-2 font-mono text-xs text-foreground outline-none focus:border-state-selected-border"
                    placeholder="粘贴 gateway reasoning analytics JSON / export JSON，可导入到后端样本表"
                  />
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={async () => {
                      const report = await analyticsImportMutation.mutateAsync({
                        sourceName: "gateway-json-import",
                        jsonText: importJsonText,
                      });
                      setImportJsonText("");
                      toast(`已导入 ${report.imported} 条，跳过 ${report.skipped} 条`);
                      void Promise.all([
                        analyticsSnapshotQuery.refetch(),
                        analyticsAnalyzeQuery.refetch(),
                      ]);
                    }}
                    disabled={!importJsonText.trim() || analyticsImportMutation.isPending}
                  >
                    导入 JSON
                  </Button>
                </div>
              </AnalyticsDataCard>
            </div>

            <Card className="space-y-3">
              <div className="flex items-center gap-2">
                <Activity className="h-3.5 w-3.5 text-muted-foreground" />
                <h2 className="text-base font-semibold text-foreground">Gateway 行为映射</h2>
              </div>
              <div className="grid grid-cols-1 gap-2 text-xs text-muted-foreground md:grid-cols-3">
                <div>Token 模式：沿用 reasoning_tokens、模型规则、预算与 provider failover。</div>
                <div>Final-only 模式：high/xhigh 且 final answer only 时进入同一预算系统。</div>
                <div>Context compaction：符合压缩信号时豁免拦截，不消耗预算。</div>
              </div>
            </Card>
          </div>
        )}
      </div>
    </div>
  );
}
