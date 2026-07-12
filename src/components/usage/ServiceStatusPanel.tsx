// Usage: Compact status.input.im dynamic model status panel.

import { AlertCircle, RefreshCw } from "lucide-react";
import { useServiceStatusQuery } from "../../query/serviceStatus";
import { type ServiceStatusCellKind } from "../../services/usage/serviceStatus";
import { Button } from "../../ui/Button";
import { EmptyState } from "../../ui/EmptyState";
import { Spinner } from "../../ui/Spinner";
import { cn } from "../../utils/cn";
import { formatPercent } from "../../utils/formatters";

const REFRESH_INTERVAL_MS = 60_000;
const HISTORY_CELL_COUNT = 60;

function kindClass(kind: ServiceStatusCellKind) {
  switch (kind) {
    case "green":
      return "bg-emerald-400";
    case "yellow":
      return "bg-yellow-300";
    case "red":
      return "bg-red-500";
    case "gray":
      return "bg-zinc-600";
  }
}

function kindText(kind: ServiceStatusCellKind): string {
  switch (kind) {
    case "green":
      return "正常";
    case "yellow":
      return "高延迟";
    case "red":
      return "失败";
    case "gray":
      return "缺少数据";
  }
}

function formatTs(ts: number | null | undefined) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleString();
}

function shortTime(ts: number | null | undefined) {
  if (!ts) return "--";
  return new Date(ts * 1000).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function statusTextClass(kind: ServiceStatusCellKind) {
  switch (kind) {
    case "green":
      return "text-emerald-300";
    case "yellow":
      return "text-yellow-300";
    case "red":
      return "text-red-400";
    case "gray":
      return "text-zinc-400";
  }
}

function HistoryBars({
  history,
}: {
  history: { kind: ServiceStatusCellKind; latency_ms: number | null; error: string | null }[];
}) {
  const recent = history.slice(-HISTORY_CELL_COUNT);
  const missing = Math.max(0, HISTORY_CELL_COUNT - recent.length);
  const cells = [
    ...Array.from({ length: missing }, () => ({
      kind: "gray" as const,
      latency_ms: null,
      error: null,
    })),
    ...recent,
  ];

  return (
    <div
      className="grid h-8 w-full gap-1"
      style={{
        gridTemplateColumns: `repeat(${HISTORY_CELL_COUNT}, minmax(4px, 10px))`,
        justifyContent: "space-between",
      }}
      aria-label="最近 60 分钟服务状态"
    >
      {cells.map((cell, index) => (
        <div
          key={index}
          className={cn(
            "h-full rounded-[3px] shadow-[0_0_10px_rgba(255,255,255,0.04)]",
            kindClass(cell.kind)
          )}
          title={`${index - HISTORY_CELL_COUNT + 1}m · ${kindText(cell.kind)}${
            cell.latency_ms != null ? ` · ${cell.latency_ms} ms` : ""
          }${cell.error ? ` · ${cell.error}` : ""}`}
        />
      ))}
    </div>
  );
}

function TimeAxis() {
  return (
    <div className="mt-2 grid grid-cols-5 text-xs font-bold text-zinc-400">
      <span>-60m</span>
      <span className="text-center">-45m</span>
      <span className="text-center">-30m</span>
      <span className="text-center">-15m</span>
      <span className="text-right">现在</span>
    </div>
  );
}

function ServiceStatusRow({
  model,
  service,
}: {
  model: string;
  service:
    | {
        uptime_pct: number | null;
        latest_kind: ServiceStatusCellKind;
        history: {
          kind: ServiceStatusCellKind;
          latency_ms: number | null;
          error: string | null;
        }[];
      }
    | undefined;
}) {
  const kind = service?.latest_kind ?? "gray";
  const sampleCount = Math.min(service?.history.length ?? 0, HISTORY_CELL_COUNT);

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <div className="flex min-w-32 items-center gap-2">
          <span className="font-mono text-base font-black tracking-normal text-zinc-100">
            {model}
          </span>
          <span className={cn("h-3 w-3 rounded-full shadow-sm", kindClass(kind))} />
          <span className={cn("text-sm font-bold", statusTextClass(kind))}>{kindText(kind)}</span>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-8 gap-y-1 text-sm">
        <div className="flex items-center gap-3">
          <span className="font-bold text-zinc-400">可用率</span>
          <span
            className={cn(
              "font-mono text-base font-black tracking-normal",
              service?.uptime_pct == null ? "text-zinc-400" : statusTextClass(kind)
            )}
          >
            {service?.uptime_pct != null ? formatPercent(service.uptime_pct / 100, 2) : "--"}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="font-bold text-zinc-400">样本</span>
          <span className="font-mono text-base font-black tracking-normal text-zinc-100">
            {sampleCount}/{HISTORY_CELL_COUNT}
          </span>
        </div>
      </div>

      <HistoryBars history={service?.history ?? []} />
      <TimeAxis />
    </div>
  );
}

export function ServiceStatusPanel({ enabled }: { enabled: boolean }) {
  const query = useServiceStatusQuery({
    enabled,
    refetchIntervalMs: enabled ? REFRESH_INTERVAL_MS : false,
  });
  const result = query.data;
  const snapshot = result?.snapshot ?? null;
  const services = snapshot?.response.services ?? [];
  const refreshing = query.isFetching && !query.isLoading;
  const generatedAt = snapshot?.response.generated_at ?? null;

  return (
    <div className="relative overflow-hidden rounded-lg border border-zinc-700/70 bg-[#171b20] p-4 text-zinc-100 shadow-sm">
      <div className="pointer-events-none absolute inset-y-0 right-[22%] w-28 bg-zinc-100/10 blur-2xl" />
      <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(circle_at_20%_0%,rgba(255,255,255,0.08),transparent_34%)]" />
      <div className="relative">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold text-zinc-100">模型服务状态</h3>
            <div className="mt-1 text-xs text-zinc-400">
              status.input.im · 最近 60 分钟 · 60 秒轮询
            </div>
          </div>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void query.refetch()}
            disabled={query.isFetching}
            className="border-zinc-600 bg-zinc-800/80 text-zinc-100 hover:bg-zinc-700"
          >
            <RefreshCw className={cn("h-4 w-4", query.isFetching && "animate-spin")} />
            刷新
          </Button>
        </div>

        {query.isLoading ? (
          <div className="flex items-center gap-2 text-sm text-zinc-400">
            <Spinner size="sm" />
            加载模型服务状态中...
          </div>
        ) : result?.error || query.error ? (
          <div className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/30 dark:text-amber-200">
            <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
            <span>{result?.error ?? query.error?.message ?? "状态请求失败"}</span>
          </div>
        ) : services.length === 0 ? (
          <EmptyState
            variant="dashed"
            title="暂无模型状态"
            description="status.input.im 当前没有返回可展示的监测项目。"
          />
        ) : (
          <div className={cn("space-y-6", refreshing && "opacity-70")}>
            {services.map((service) => (
              <ServiceStatusRow key={service.model} model={service.model} service={service} />
            ))}
            <div className="flex flex-wrap items-center gap-x-8 gap-y-1 pt-1 text-sm font-bold text-zinc-300">
              <span>接口生成 {shortTime(generatedAt)}</span>
              <span>状态刷新 {shortTime(snapshot?.refreshed_at)}</span>
              <span className="text-xs font-normal text-zinc-500">
                {formatTs(snapshot?.refreshed_at)}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
