// Usage: Remote sub2api usage snapshot panel shown inside Usage page.

import { useMemo, useState } from "react";
import { AlertCircle, CheckCircle2, Edit3, Plus, RefreshCw, Trash2, WifiOff } from "lucide-react";
import {
  useRemoteUsageCustomSourceDeleteMutation,
  useRemoteUsageCustomSourceEnabledMutation,
  useRemoteUsageCustomSourceUpsertMutation,
  useRemoteUsageSnapshotsQuery,
  useRemoteUsageSourcesQuery,
} from "../../query/remoteUsage";
import type { CliKey } from "../../services/providers/providers";
import type {
  RemoteUsageSnapshotRow,
  RemoteUsageSourceSummary,
} from "../../services/usage/remoteUsage";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { EmptyState } from "../../ui/EmptyState";
import { Input } from "../../ui/Input";
import { Spinner } from "../../ui/Spinner";
import { Switch } from "../../ui/Switch";
import { cn } from "../../utils/cn";
import { formatInteger } from "../../utils/formatters";

const POLL_INTERVAL_MS = 60_000;
const EMPTY_SOURCES: RemoteUsageSourceSummary[] = [];

type SourceTypeFilter = "all" | "provider" | "custom";

function formatMoney(value: number | null | undefined, unit?: string | null) {
  if (value == null || !Number.isFinite(value)) return "-";
  const suffix = unit?.trim() || "USD";
  return `${value.toLocaleString(undefined, {
    maximumFractionDigits: 4,
  })} ${suffix}`;
}

function formatNumber(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value)) return "-";
  return value.toLocaleString(undefined, { maximumFractionDigits: 2 });
}

function formatTime(ts: number | null | undefined) {
  if (!ts) return "从未成功";
  return new Date(ts * 1000).toLocaleString();
}

function statusLabel(status: RemoteUsageSnapshotRow["status"]) {
  switch (status) {
    case "fresh":
      return "已刷新";
    case "stale":
      return "缓存";
    case "unauthorized":
      return "鉴权失败";
    case "not_configured":
      return "未配置";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

function StatusPill({ row }: { row: RemoteUsageSnapshotRow }) {
  const fresh = row.status === "fresh";
  const stale = row.status === "stale";
  const unauthorized = row.status === "unauthorized";
  return (
    <span
      className={cn(
        "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-xs font-medium",
        fresh &&
          "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900/50 dark:bg-emerald-950/30 dark:text-emerald-300",
        stale &&
          "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-300",
        unauthorized &&
          "border-red-200 bg-red-50 text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300",
        !fresh && !stale && !unauthorized && "border-border bg-secondary text-muted-foreground"
      )}
    >
      {fresh ? <CheckCircle2 className="h-3.5 w-3.5" /> : <AlertCircle className="h-3.5 w-3.5" />}
      {statusLabel(row.status)}
    </span>
  );
}

function SourceBadges({ source }: { source: RemoteUsageSourceSummary }) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="rounded border border-border bg-secondary px-1.5 py-0.5 text-[11px] text-muted-foreground">
        {source.source_type === "provider" ? "Provider" : "Custom"}
      </span>
      <span className="rounded border border-border bg-secondary px-1.5 py-0.5 text-[11px] text-muted-foreground">
        {source.cli_key}
      </span>
      {!source.enabled && (
        <span className="rounded border border-border bg-muted px-1.5 py-0.5 text-[11px] text-muted-foreground">
          已禁用
        </span>
      )}
    </div>
  );
}

function UsageMetric({
  label,
  cost,
  tokens,
  requests,
  unit,
}: {
  label: string;
  cost?: number | null;
  tokens?: number | null;
  requests?: number | null;
  unit?: string | null;
}) {
  return (
    <div className="rounded-md border border-border bg-background px-3 py-2">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-sm font-semibold text-foreground">{formatMoney(cost, unit)}</div>
      <div className="mt-1 text-[11px] text-muted-foreground">
        {formatNumber(tokens)} tokens · {formatNumber(requests)} req
      </div>
    </div>
  );
}

function RemoteUsageRowCard({ row }: { row: RemoteUsageSnapshotRow }) {
  const snapshot = row.snapshot;
  const usage = snapshot?.usage;
  return (
    <div className="rounded-lg border border-border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-card-foreground">
              {row.source.name}
            </h3>
            <StatusPill row={row} />
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {row.source.endpoint_url}
          </div>
          <div className="mt-2">
            <SourceBadges source={row.source} />
          </div>
        </div>
        <div className="text-right text-xs text-muted-foreground">
          <div>上次成功</div>
          <div className="mt-1 text-card-foreground">
            {formatTime(row.last_successful_refresh_at)}
          </div>
        </div>
      </div>

      {row.last_error ? (
        <div className="mt-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-200">
          {row.last_error}
        </div>
      ) : null}

      {snapshot ? (
        <>
          <div className="mt-4 grid gap-3 md:grid-cols-4">
            <div className="rounded-md border border-border bg-background px-3 py-2">
              <div className="text-xs text-muted-foreground">余额</div>
              <div className="mt-1 text-sm font-semibold text-foreground">
                {formatMoney(snapshot.remaining, snapshot.unit)}
              </div>
              <div className="mt-1 truncate text-[11px] text-muted-foreground">
                {snapshot.plan_name || "未返回套餐"} · {snapshot.subscription || "无订阅信息"}
              </div>
            </div>
            <UsageMetric
              label="今日"
              cost={usage?.today?.cost}
              tokens={usage?.today?.tokens}
              requests={usage?.today?.requests}
              unit={snapshot.unit}
            />
            <UsageMetric
              label="本周"
              cost={usage?.week?.cost}
              tokens={usage?.week?.tokens}
              requests={usage?.week?.requests}
              unit={snapshot.unit}
            />
            <UsageMetric
              label="本月"
              cost={usage?.month?.cost}
              tokens={usage?.month?.tokens}
              requests={usage?.month?.requests}
              unit={snapshot.unit}
            />
          </div>
          {snapshot.model_stats.length > 0 ? (
            <div className="mt-4 overflow-hidden rounded-md border border-border">
              <table className="w-full text-left text-xs">
                <thead className="bg-secondary text-muted-foreground">
                  <tr>
                    <th className="px-3 py-2 font-medium">模型</th>
                    <th className="px-3 py-2 font-medium">成本</th>
                    <th className="px-3 py-2 font-medium">Tokens</th>
                    <th className="px-3 py-2 font-medium">请求</th>
                  </tr>
                </thead>
                <tbody>
                  {snapshot.model_stats.slice(0, 8).map((item) => (
                    <tr key={item.model} className="border-t border-border">
                      <td className="max-w-80 truncate px-3 py-2 text-card-foreground">
                        {item.model}
                      </td>
                      <td className="px-3 py-2">{formatMoney(item.cost, snapshot.unit)}</td>
                      <td className="px-3 py-2">{formatNumber(item.tokens)}</td>
                      <td className="px-3 py-2">{formatNumber(item.requests)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function SourceManagerDialog({
  open,
  onOpenChange,
  sources,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sources: RemoteUsageSourceSummary[];
}) {
  const [editing, setEditing] = useState<RemoteUsageSourceSummary | null>(null);
  const [cliKey, setCliKey] = useState<CliKey>("codex");
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(true);
  const upsert = useRemoteUsageCustomSourceUpsertMutation();
  const remove = useRemoteUsageCustomSourceDeleteMutation();
  const setEnabledMutation = useRemoteUsageCustomSourceEnabledMutation();

  function resetForm(source?: RemoteUsageSourceSummary | null) {
    setEditing(source ?? null);
    setCliKey(source?.cli_key ?? "codex");
    setName(source?.name ?? "");
    setBaseUrl(source?.base_url ?? "");
    setApiKey("");
    setEnabled(source?.enabled ?? true);
  }

  async function submit() {
    await upsert.mutateAsync({
      id: editing?.custom_source_id ?? null,
      cliKey,
      name,
      baseUrl,
      apiKey: apiKey.trim() ? apiKey : null,
      enabled,
    });
    resetForm(null);
  }

  const customSources = sources.filter((source) => source.source_type === "custom");
  const busy = upsert.isPending || remove.isPending || setEnabledMutation.isPending;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next);
        if (!next) resetForm(null);
      }}
      title="远端用量来源"
      description="独立 sub2api 来源存储在本地数据库，API Key 不会返回前端。"
      className="max-w-3xl"
    >
      <div className="grid gap-4 md:grid-cols-[1fr_1.1fr]">
        <div className="space-y-2">
          <div className="text-xs font-medium text-muted-foreground">Custom 来源</div>
          {customSources.length === 0 ? (
            <EmptyState
              variant="dashed"
              title="暂无独立来源"
              description="Provider 来源会自动读取，无需在这里添加。"
            />
          ) : (
            customSources.map((source) => (
              <div
                key={source.source_id}
                className="flex items-center justify-between gap-3 rounded-md border border-border p-3"
              >
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-foreground">{source.name}</div>
                  <div className="truncate text-xs text-muted-foreground">{source.base_url}</div>
                </div>
                <div className="flex items-center gap-1">
                  <Switch
                    size="sm"
                    checked={source.enabled}
                    onCheckedChange={(checked) => {
                      if (source.custom_source_id != null) {
                        setEnabledMutation.mutate({
                          id: source.custom_source_id,
                          enabled: checked,
                        });
                      }
                    }}
                    aria-label="启用来源"
                  />
                  <Button size="icon" variant="secondary" onClick={() => resetForm(source)}>
                    <Edit3 className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    size="icon"
                    variant="secondary"
                    onClick={() => {
                      if (source.custom_source_id != null) remove.mutate(source.custom_source_id);
                    }}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            ))
          )}
        </div>
        <div className="rounded-md border border-border p-4">
          <div className="mb-3 flex items-center justify-between">
            <div className="text-sm font-medium text-foreground">
              {editing ? "编辑来源" : "新增来源"}
            </div>
            {editing ? (
              <Button size="sm" variant="secondary" onClick={() => resetForm(null)}>
                新增
              </Button>
            ) : null}
          </div>
          <div className="space-y-3">
            <label className="block">
              <span className="text-xs text-muted-foreground">CLI</span>
              <select
                value={cliKey}
                onChange={(event) => setCliKey(event.currentTarget.value as CliKey)}
                className="mt-1 h-9 w-full rounded-md border border-border bg-white px-2 text-sm dark:bg-secondary"
              >
                <option value="claude">claude</option>
                <option value="codex">codex</option>
                <option value="gemini">gemini</option>
              </select>
            </label>
            <label className="block">
              <span className="text-xs text-muted-foreground">名称</span>
              <Input value={name} onChange={(event) => setName(event.currentTarget.value)} />
            </label>
            <label className="block">
              <span className="text-xs text-muted-foreground">Base URL</span>
              <Input value={baseUrl} onChange={(event) => setBaseUrl(event.currentTarget.value)} />
            </label>
            <label className="block">
              <span className="text-xs text-muted-foreground">
                API Key{editing ? "（留空保持不变）" : ""}
              </span>
              <Input
                type="password"
                value={apiKey}
                onChange={(event) => setApiKey(event.currentTarget.value)}
              />
            </label>
            <label className="flex items-center gap-2 text-sm">
              <Switch checked={enabled} onCheckedChange={setEnabled} />
              启用
            </label>
            <Button onClick={submit} disabled={busy || !name.trim() || !baseUrl.trim()}>
              保存
            </Button>
          </div>
        </div>
      </div>
    </Dialog>
  );
}

export function RemoteUsagePanel({ cliKey }: { cliKey: CliKey | null }) {
  const [sourceType, setSourceType] = useState<SourceTypeFilter>("all");
  const [managerOpen, setManagerOpen] = useState(false);
  const sourcesQuery = useRemoteUsageSourcesQuery(cliKey, { refetchIntervalMs: POLL_INTERVAL_MS });
  const sources = sourcesQuery.data ?? EMPTY_SOURCES;
  const filteredSources = useMemo(
    () => sources.filter((source) => sourceType === "all" || source.source_type === sourceType),
    [sources, sourceType]
  );
  const sourceIds = filteredSources.map((source) => source.source_id);
  const snapshotsQuery = useRemoteUsageSnapshotsQuery(
    { cliKey, sourceIds },
    {
      enabled: filteredSources.length > 0,
      refetchIntervalMs: POLL_INTERVAL_MS,
    }
  );
  const rows = snapshotsQuery.data ?? [];
  const loading = sourcesQuery.isLoading || snapshotsQuery.isLoading;
  const refreshing = sourcesQuery.isFetching || snapshotsQuery.isFetching;

  return (
    <div className="px-6 pb-6">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-sm font-semibold text-foreground">远端 sub2api 用量</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {formatInteger(filteredSources.length)} 来源 · {refreshing ? "刷新中" : "轮询 60s"}
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <select
            value={sourceType}
            onChange={(event) => setSourceType(event.currentTarget.value as SourceTypeFilter)}
            className="h-8 rounded-md border border-border bg-white px-2 text-xs dark:bg-secondary"
          >
            <option value="all">全部来源</option>
            <option value="provider">Provider</option>
            <option value="custom">Custom</option>
          </select>
          <Button size="sm" variant="secondary" onClick={() => setManagerOpen(true)}>
            <Plus className="h-4 w-4" />
            来源
          </Button>
          <Button
            size="sm"
            onClick={() => {
              void sourcesQuery.refetch();
              void snapshotsQuery.refetch();
            }}
            disabled={refreshing}
          >
            <RefreshCw className={cn("h-4 w-4", refreshing && "animate-spin")} />
            刷新
          </Button>
        </div>
      </div>

      {sourcesQuery.error || snapshotsQuery.error ? (
        <div className="mb-4 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {(sourcesQuery.error ?? snapshotsQuery.error)?.message}
        </div>
      ) : null}

      {loading ? (
        <div className="flex min-h-48 items-center justify-center">
          <Spinner />
        </div>
      ) : filteredSources.length === 0 ? (
        <EmptyState
          variant="dashed"
          icon={<WifiOff className="h-5 w-5" />}
          title="暂无可用远端来源"
          description="配置 API Key 模式 Provider，或添加独立 custom 来源。"
          action={
            <Button size="sm" onClick={() => setManagerOpen(true)}>
              <Plus className="h-4 w-4" />
              添加来源
            </Button>
          }
        />
      ) : (
        <div className="space-y-3">
          {rows.map((row) => (
            <RemoteUsageRowCard key={row.source.source_id} row={row} />
          ))}
        </div>
      )}

      <SourceManagerDialog open={managerOpen} onOpenChange={setManagerOpen} sources={sources} />
    </div>
  );
}
