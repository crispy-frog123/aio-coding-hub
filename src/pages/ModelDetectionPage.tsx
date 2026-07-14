import { useMemo, useRef, useState } from "react";
import {
  CheckCircle2,
  CircleDashed,
  Clock3,
  ListRestart,
  Play,
  Plus,
  Search,
  ServerCog,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";
import {
  useProviderModelProbeMutation,
  useProviderModelsListMutation,
} from "../query/providerModels";
import { useProvidersListQuery } from "../query/providers";
import type { CliKey, ProviderSummary } from "../services/providers/providers";
import {
  validateProviderModelId,
  type ProviderModelInfo,
  type ProviderModelProbeResult,
  type ProviderModelsResult,
} from "../services/providers/providerModels";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Input } from "../ui/Input";
import { PageHeader } from "../ui/PageHeader";
import { Select } from "../ui/Select";
import { Spinner } from "../ui/Spinner";
import { TabList } from "../ui/TabList";
import { cn } from "../utils/cn";

type ProbeFilter = "all" | "untested" | "available" | "unavailable";

const CLI_TABS: Array<{ key: CliKey; label: string }> = [
  { key: "codex", label: "OpenAI 兼容" },
  { key: "claude", label: "Anthropic 兼容" },
  { key: "gemini", label: "Gemini" },
];

const OUTCOME_LABELS: Record<string, string> = {
  available: "可用",
  rate_limited: "已限流",
  auth_error: "认证失败",
  model_unavailable: "模型不可用",
  request_failed: "请求失败",
  timeout: "超时",
  network_error: "网络错误",
};

function outcomeStyle(result: ProviderModelProbeResult | undefined) {
  if (!result) return "border-border bg-muted text-muted-foreground";
  if (result.ok) {
    return "border-emerald-300/70 bg-emerald-50 text-emerald-700 dark:border-emerald-700/60 dark:bg-emerald-900/25 dark:text-emerald-300";
  }
  if (result.outcome === "rate_limited") {
    return "border-amber-300/70 bg-amber-50 text-amber-700 dark:border-amber-700/60 dark:bg-amber-900/25 dark:text-amber-300";
  }
  return "border-rose-300/70 bg-rose-50 text-rose-700 dark:border-rose-700/60 dark:bg-rose-900/25 dark:text-rose-300";
}

function formatProtocol(protocol: string) {
  if (protocol === "responses") return "Responses";
  if (protocol === "chat_completions") return "Chat Completions";
  if (protocol === "messages") return "Messages";
  if (protocol === "generate_content") return "Generate Content";
  return protocol;
}

function combineModels(
  result: ProviderModelsResult | null,
  manualModelIds: readonly string[]
): Array<ProviderModelInfo & { source: "provider" | "manual" }> {
  const seen = new Set<string>();
  const combined: Array<ProviderModelInfo & { source: "provider" | "manual" }> = [];
  for (const model of result?.models ?? []) {
    if (seen.has(model.id)) continue;
    seen.add(model.id);
    combined.push({ ...model, source: "provider" });
  }
  for (const id of manualModelIds) {
    if (seen.has(id)) continue;
    seen.add(id);
    combined.push({
      id,
      display_name: null,
      owned_by: null,
      model_type: null,
      supported_methods: [],
      source: "manual",
    });
  }
  return combined;
}

function StatCell({ label, value, tone }: { label: string; value: number; tone?: string }) {
  return (
    <Card padding="sm" variant="inset" className="min-h-[78px] rounded-lg">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("mt-2 text-2xl font-semibold tabular-nums text-foreground", tone)}>
        {value}
      </div>
    </Card>
  );
}

export function ModelDetectionPage() {
  const [activeCli, setActiveCli] = useState<CliKey>("codex");
  const [providerId, setProviderId] = useState<number | null>(null);
  const [baseUrl, setBaseUrl] = useState<string | null>(null);
  const [listResult, setListResult] = useState<ProviderModelsResult | null>(null);
  const [manualModelIds, setManualModelIds] = useState<string[]>([]);
  const [manualModelText, setManualModelText] = useState("");
  const [searchText, setSearchText] = useState("");
  const [probeFilter, setProbeFilter] = useState<ProbeFilter>("all");
  const [probeResults, setProbeResults] = useState<Record<string, ProviderModelProbeResult>>({});
  const [probingModels, setProbingModels] = useState<Set<string>>(() => new Set());
  const selectionVersionRef = useRef(0);

  const providersQuery = useProvidersListQuery(activeCli);
  const listMutation = useProviderModelsListMutation();
  const probeMutation = useProviderModelProbeMutation();
  const providers = providersQuery.data ?? [];
  const selectedProvider = providers.find((provider) => provider.id === providerId) ?? null;
  const models = useMemo(
    () => combineModels(listResult, manualModelIds),
    [listResult, manualModelIds]
  );
  const filteredModels = useMemo(() => {
    const needle = searchText.trim().toLocaleLowerCase();
    return models.filter((model) => {
      const result = probeResults[model.id];
      const matchesText =
        !needle ||
        model.id.toLocaleLowerCase().includes(needle) ||
        model.display_name?.toLocaleLowerCase().includes(needle) ||
        model.owned_by?.toLocaleLowerCase().includes(needle);
      if (!matchesText) return false;
      if (probeFilter === "untested") return !result;
      if (probeFilter === "available") return result?.ok === true;
      if (probeFilter === "unavailable") return Boolean(result && !result.ok);
      return true;
    });
  }, [models, probeFilter, probeResults, searchText]);

  const testedCount = Object.keys(probeResults).filter((id) =>
    models.some((model) => model.id === id)
  ).length;
  const availableCount = Object.values(probeResults).filter((result) => result.ok).length;
  const unavailableCount = testedCount - availableCount;

  function resetProviderState(nextProvider: ProviderSummary | null) {
    selectionVersionRef.current += 1;
    setProviderId(nextProvider?.id ?? null);
    setBaseUrl(nextProvider?.base_urls[0] ?? null);
    setListResult(null);
    setManualModelIds([]);
    setManualModelText("");
    setProbeResults({});
    setProbingModels(new Set());
  }

  function handleCliChange(nextCli: CliKey) {
    setActiveCli(nextCli);
    resetProviderState(null);
  }

  function handleProviderChange(value: string) {
    const nextId = Number(value);
    const nextProvider = providers.find((provider) => provider.id === nextId) ?? null;
    resetProviderState(nextProvider);
  }

  async function fetchModels() {
    if (!selectedProvider) return;
    const selectionVersion = selectionVersionRef.current;
    try {
      const result = await listMutation.mutateAsync({
        providerId: selectedProvider.id,
        baseUrl,
      });
      if (selectionVersionRef.current !== selectionVersion) return;
      setListResult(result);
      setProbeResults({});
      if (result.ok) {
        toast(`${selectedProvider.name}: 获取到 ${result.models.length} 个模型`);
      } else {
        toast(`${selectedProvider.name}: ${result.error ?? "模型列表获取失败"}`);
      }
    } catch (error) {
      if (selectionVersionRef.current !== selectionVersion) return;
      toast(`获取模型列表失败：${String(error)}`);
    }
  }

  function addManualModel() {
    try {
      const model = validateProviderModelId(manualModelText);
      if (!models.some((item) => item.id === model)) {
        setManualModelIds((current) => [...current, model]);
      }
      setManualModelText("");
    } catch (error) {
      toast(String(error));
    }
  }

  async function probeModel(model: string) {
    if (!selectedProvider || probingModels.has(model)) return;
    const selectionVersion = selectionVersionRef.current;
    setProbingModels((current) => new Set(current).add(model));
    try {
      const result = await probeMutation.mutateAsync({
        providerId: selectedProvider.id,
        model,
        baseUrl,
      });
      if (selectionVersionRef.current !== selectionVersion) return;
      setProbeResults((current) => ({ ...current, [model]: result }));
      toast(`${model}: ${OUTCOME_LABELS[result.outcome] ?? result.outcome}`);
    } catch (error) {
      if (selectionVersionRef.current !== selectionVersion) return;
      toast(`检测 ${model} 失败：${String(error)}`);
    } finally {
      if (selectionVersionRef.current !== selectionVersion) return;
      setProbingModels((current) => {
        const next = new Set(current);
        next.delete(model);
        return next;
      });
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-hidden">
      <PageHeader
        title="模型检测"
        actions={
          <TabList
            ariaLabel="Provider 协议"
            items={CLI_TABS}
            value={activeCli}
            onChange={handleCliChange}
          />
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto pr-1 scrollbar-overlay">
        <div className="space-y-4 pb-5">
          <Card padding="sm" className="rounded-lg">
            <div className="grid gap-3 lg:grid-cols-[minmax(220px,1fr)_minmax(240px,1.4fr)_auto] lg:items-end">
              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">Provider</span>
                <Select
                  value={providerId?.toString() ?? ""}
                  onChange={(event) => handleProviderChange(event.target.value)}
                  disabled={providersQuery.isLoading}
                >
                  <option value="">选择 Provider</option>
                  {providers.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} · {provider.auth_mode === "oauth" ? "OAuth" : "API Key"}
                    </option>
                  ))}
                </Select>
              </label>

              <label className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground">Base URL</span>
                <Select
                  value={baseUrl ?? ""}
                  onChange={(event) => {
                    selectionVersionRef.current += 1;
                    setBaseUrl(event.target.value || null);
                    setListResult(null);
                    setProbeResults({});
                    setProbingModels(new Set());
                  }}
                  disabled={!selectedProvider}
                  mono
                >
                  {selectedProvider?.base_urls.length ? null : (
                    <option value="">OAuth 默认地址</option>
                  )}
                  {(selectedProvider?.base_urls ?? []).map((url) => (
                    <option key={url} value={url}>
                      {url}
                    </option>
                  ))}
                </Select>
              </label>

              <Button
                variant="primary"
                onClick={() => void fetchModels()}
                disabled={!selectedProvider || listMutation.isPending}
                className="h-10"
              >
                {listMutation.isPending ? (
                  <Spinner size="sm" />
                ) : (
                  <ListRestart className="h-4 w-4" />
                )}
                获取模型列表
              </Button>
            </div>

            {listResult ? (
              <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-border/70 pt-3 text-xs text-muted-foreground">
                <Badge variant="outline" className="font-mono">
                  HTTP {listResult.status ?? "-"}
                </Badge>
                <span>{listResult.latency_ms} ms</span>
                <span className="min-w-0 truncate font-mono" title={listResult.endpoint}>
                  {listResult.endpoint}
                </span>
              </div>
            ) : null}
          </Card>

          {listResult && !listResult.ok ? (
            <div className="rounded-lg border border-rose-300/60 bg-rose-50 px-4 py-3 text-sm text-rose-700 dark:border-rose-800/60 dark:bg-rose-950/30 dark:text-rose-300">
              <div className="font-medium">{listResult.error ?? "模型列表获取失败"}</div>
              {listResult.response_preview ? (
                <div className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-all font-mono text-xs">
                  {listResult.response_preview}
                </div>
              ) : null}
            </div>
          ) : null}

          <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
            <StatCell label="模型总数" value={models.length} />
            <StatCell label="已检测" value={testedCount} />
            <StatCell
              label="可用"
              value={availableCount}
              tone="text-emerald-600 dark:text-emerald-400"
            />
            <StatCell
              label="未通过"
              value={unavailableCount}
              tone="text-rose-600 dark:text-rose-400"
            />
          </div>

          <Card padding="none" className="rounded-lg">
            <div className="flex flex-col gap-3 border-b border-border px-4 py-3 xl:flex-row xl:items-center xl:justify-between">
              <div className="flex min-w-0 flex-1 flex-col gap-2 sm:flex-row">
                <div className="relative min-w-0 flex-1">
                  <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    value={searchText}
                    onChange={(event) => setSearchText(event.target.value)}
                    placeholder="搜索模型 ID"
                    className="pl-9"
                  />
                </div>
                <Select
                  value={probeFilter}
                  onChange={(event) => setProbeFilter(event.target.value as ProbeFilter)}
                  className="sm:w-36"
                >
                  <option value="all">全部状态</option>
                  <option value="untested">未检测</option>
                  <option value="available">可用</option>
                  <option value="unavailable">未通过</option>
                </Select>
              </div>

              <div className="flex min-w-0 gap-2 sm:min-w-[300px]">
                <Input
                  value={manualModelText}
                  onChange={(event) => setManualModelText(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") addManualModel();
                  }}
                  placeholder="手动输入模型 ID"
                  className="min-w-0"
                  disabled={!selectedProvider}
                />
                <Button
                  size="icon"
                  onClick={addManualModel}
                  disabled={!selectedProvider || !manualModelText.trim()}
                  title="添加模型"
                  aria-label="添加模型"
                >
                  <Plus className="h-4 w-4" />
                </Button>
              </div>
            </div>

            {filteredModels.length === 0 ? (
              <EmptyState
                icon={<ServerCog className="h-8 w-8" />}
                title={models.length === 0 ? "暂无模型" : "没有匹配的模型"}
                className="min-h-52"
              />
            ) : (
              <div className="overflow-x-auto">
                <div className="min-w-[860px]">
                  <div className="grid grid-cols-[minmax(250px,1.4fr)_140px_170px_110px_96px] gap-3 border-b border-border bg-muted/40 px-4 py-2 text-xs font-medium text-muted-foreground">
                    <div>模型</div>
                    <div>来源</div>
                    <div>检测结果</div>
                    <div>延迟</div>
                    <div className="text-right">操作</div>
                  </div>
                  {filteredModels.map((model) => {
                    const result = probeResults[model.id];
                    const pending = probingModels.has(model.id);
                    return (
                      <div
                        key={model.id}
                        className="grid min-h-[68px] grid-cols-[minmax(250px,1.4fr)_140px_170px_110px_96px] items-center gap-3 border-b border-border/70 px-4 py-3 last:border-b-0"
                      >
                        <div className="min-w-0">
                          <div
                            className="truncate font-mono text-sm font-medium text-foreground"
                            title={model.id}
                          >
                            {model.id}
                          </div>
                          <div className="mt-1 truncate text-xs text-muted-foreground">
                            {[model.display_name, model.owned_by].filter(Boolean).join(" · ") ||
                              "-"}
                          </div>
                        </div>
                        <div>
                          <Badge variant="outline">
                            {model.source === "provider" ? "接口返回" : "手动添加"}
                          </Badge>
                        </div>
                        <div>
                          {pending ? (
                            <div className="inline-flex items-center gap-2 text-xs text-muted-foreground">
                              <Spinner size="sm" /> 检测中
                            </div>
                          ) : result ? (
                            <div
                              className={cn(
                                "inline-flex rounded-md border px-2 py-1 text-xs font-medium",
                                outcomeStyle(result)
                              )}
                              title={result.error ?? result.endpoint}
                            >
                              {OUTCOME_LABELS[result.outcome] ?? result.outcome}
                              <span className="ml-1 opacity-70">
                                · {formatProtocol(result.protocol)}
                              </span>
                            </div>
                          ) : (
                            <div className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                              <CircleDashed className="h-3.5 w-3.5" /> 未检测
                            </div>
                          )}
                        </div>
                        <div className="text-xs text-muted-foreground">
                          {result ? (
                            <span className="inline-flex items-center gap-1">
                              <Clock3 className="h-3.5 w-3.5" /> {result.latency_ms} ms
                            </span>
                          ) : (
                            "-"
                          )}
                        </div>
                        <div className="text-right">
                          <Button
                            size="sm"
                            variant={result?.ok ? "secondary" : "primary"}
                            onClick={() => void probeModel(model.id)}
                            disabled={!selectedProvider || pending}
                          >
                            {pending ? (
                              <Spinner size="sm" />
                            ) : result?.ok ? (
                              <CheckCircle2 className="h-3.5 w-3.5" />
                            ) : result ? (
                              <XCircle className="h-3.5 w-3.5" />
                            ) : (
                              <Play className="h-3.5 w-3.5" />
                            )}
                            检测
                          </Button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
