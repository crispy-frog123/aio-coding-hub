// Usage: Frontend service wrapper for remote sub2api usage snapshots.

import {
  commands,
  type RemoteUsageCustomSourceUpsertInput as GeneratedRemoteUsageCustomSourceUpsertInput,
  type RemoteUsageRefreshInput as GeneratedRemoteUsageRefreshInput,
  type RemoteUsageSnapshotRow as GeneratedRemoteUsageSnapshotRow,
  type RemoteUsageSourceSummary as GeneratedRemoteUsageSourceSummary,
} from "../../generated/bindings";
import { invokeGeneratedIpc, mapGeneratedCommandResponse } from "../generatedIpc";
import { narrowGeneratedStringUnion, type Override } from "../generatedTypeUtils";
import type { CliKey } from "../providers/providers";
import { validateUsageCliKey } from "./usage";

const SOURCE_TYPE_VALUES = ["provider", "custom"] as const;
const SNAPSHOT_STATUS_VALUES = [
  "fresh",
  "stale",
  "unauthorized",
  "not_configured",
  "failed",
] as const;
const CLI_KEY_VALUES = ["claude", "codex", "gemini"] as const satisfies readonly CliKey[];

export type RemoteUsageSourceType = (typeof SOURCE_TYPE_VALUES)[number];
export type RemoteUsageSnapshotStatus = (typeof SNAPSHOT_STATUS_VALUES)[number];

export type RemoteUsageSourceSummary = Override<
  GeneratedRemoteUsageSourceSummary,
  {
    source_type: RemoteUsageSourceType;
    cli_key: CliKey;
  }
>;

export type RemoteUsageSnapshotRow = Override<
  GeneratedRemoteUsageSnapshotRow,
  {
    source: RemoteUsageSourceSummary;
    status: RemoteUsageSnapshotStatus;
  }
>;

export type RemoteUsageRefreshInput = Override<
  GeneratedRemoteUsageRefreshInput,
  {
    cliKey?: CliKey | null;
    sourceIds?: string[] | null;
  }
>;

export type RemoteUsageCustomSourceUpsertInput = Override<
  GeneratedRemoteUsageCustomSourceUpsertInput,
  {
    id?: number | null;
    cliKey: CliKey;
    apiKey?: string | null;
  }
>;

function normalizeText(value: string, label: string, required = true): string {
  const normalized = value.trim();
  if (required && !normalized) {
    throw new Error(`IPC_INVALID_RESULT: ${label} is required`);
  }
  return normalized;
}

function normalizeOptionalId(value: number | null | undefined, label: string): number | null {
  if (value == null) return null;
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`IPC_INVALID_RESULT: invalid ${label}=${value}`);
  }
  return value;
}

function assertNoApiKeyLeak(value: unknown) {
  if (value == null || typeof value !== "object") return;
  if ("api_key" in value || "apiKey" in value || "api_key_plaintext" in value) {
    throw new Error("IPC_INVALID_RESULT: remote usage IPC result exposed api key");
  }
}

function toSourceSummary(value: GeneratedRemoteUsageSourceSummary): RemoteUsageSourceSummary {
  assertNoApiKeyLeak(value);
  return {
    ...value,
    source_id: normalizeText(value.source_id, "remote_usage.source_id"),
    source_type: narrowGeneratedStringUnion(
      value.source_type,
      SOURCE_TYPE_VALUES,
      "remote_usage.source_type"
    ),
    cli_key: narrowGeneratedStringUnion(value.cli_key, CLI_KEY_VALUES, "remote_usage.cli_key"),
    name: normalizeText(value.name, "remote_usage.name"),
    base_url: normalizeText(value.base_url, "remote_usage.base_url"),
    endpoint_url: normalizeText(value.endpoint_url, "remote_usage.endpoint_url"),
    provider_id: normalizeOptionalId(value.provider_id, "remote_usage.provider_id"),
    custom_source_id: normalizeOptionalId(value.custom_source_id, "remote_usage.custom_source_id"),
  };
}

function toSnapshotRow(value: GeneratedRemoteUsageSnapshotRow): RemoteUsageSnapshotRow {
  assertNoApiKeyLeak(value);
  return {
    ...value,
    source: toSourceSummary(value.source),
    status: narrowGeneratedStringUnion(value.status, SNAPSHOT_STATUS_VALUES, "remote_usage.status"),
  };
}

function normalizeSourceIds(sourceIds?: readonly string[] | null): string[] | null {
  if (sourceIds == null) return null;
  const normalized = new Set<string>();
  for (const raw of sourceIds) {
    const value = raw.trim();
    if (!value) continue;
    if (!/^(provider|custom):[1-9]\d*$/.test(value)) {
      throw new Error(`SEC_INVALID_INPUT: invalid sourceId=${value}`);
    }
    normalized.add(value);
  }
  return normalized.size > 0 ? [...normalized].sort((a, b) => a.localeCompare(b)) : null;
}

export function normalizeRemoteUsageRefreshInput(
  input?: RemoteUsageRefreshInput
): Required<RemoteUsageRefreshInput> {
  return {
    cliKey: validateUsageCliKey(input?.cliKey),
    sourceIds: normalizeSourceIds(input?.sourceIds),
  };
}

function normalizeCustomSourceInput(
  input: RemoteUsageCustomSourceUpsertInput
): GeneratedRemoteUsageCustomSourceUpsertInput {
  const cliKey = validateUsageCliKey(input.cliKey);
  if (!cliKey) {
    throw new Error("SEC_INVALID_INPUT: cliKey is required");
  }
  return {
    id: input.id ?? null,
    cliKey,
    name: input.name.trim(),
    baseUrl: input.baseUrl.trim(),
    apiKey: input.apiKey == null ? null : input.apiKey,
    enabled: input.enabled,
  };
}

export async function remoteUsageSourcesList(cliKey?: CliKey | null) {
  const normalizedCliKey = validateUsageCliKey(cliKey);
  return invokeGeneratedIpc<RemoteUsageSourceSummary[]>({
    title: "读取远端用量来源失败",
    cmd: "remote_usage_sources_list",
    args: { cliKey: normalizedCliKey },
    invoke: async () =>
      mapGeneratedCommandResponse(await commands.remoteUsageSourcesList(normalizedCliKey), (rows) =>
        rows.map(toSourceSummary)
      ),
  });
}

export async function remoteUsageSnapshotsRefresh(input?: RemoteUsageRefreshInput) {
  const normalized = normalizeRemoteUsageRefreshInput(input);
  return invokeGeneratedIpc<RemoteUsageSnapshotRow[]>({
    title: "刷新远端用量失败",
    cmd: "remote_usage_snapshots_refresh",
    args: { input: normalized },
    invoke: async () =>
      mapGeneratedCommandResponse(await commands.remoteUsageSnapshotsRefresh(normalized), (rows) =>
        rows.map(toSnapshotRow)
      ),
  });
}

export async function remoteUsageCustomSourceUpsert(input: RemoteUsageCustomSourceUpsertInput) {
  const normalized = normalizeCustomSourceInput(input);
  return invokeGeneratedIpc<RemoteUsageSourceSummary>({
    title: "保存远端用量来源失败",
    cmd: "remote_usage_custom_source_upsert",
    args: { input: normalized },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.remoteUsageCustomSourceUpsert(normalized),
        toSourceSummary
      ),
  });
}

export async function remoteUsageCustomSourceDelete(id: number) {
  const normalizedId = normalizeOptionalId(id, "remote_usage.custom_source_id");
  if (normalizedId == null) {
    throw new Error("SEC_INVALID_INPUT: id is required");
  }
  return invokeGeneratedIpc<boolean>({
    title: "删除远端用量来源失败",
    cmd: "remote_usage_custom_source_delete",
    args: { id: normalizedId },
    invoke: () => commands.remoteUsageCustomSourceDelete({ id: normalizedId }),
  });
}

export async function remoteUsageCustomSourceSetEnabled(id: number, enabled: boolean) {
  const normalizedId = normalizeOptionalId(id, "remote_usage.custom_source_id");
  if (normalizedId == null) {
    throw new Error("SEC_INVALID_INPUT: id is required");
  }
  return invokeGeneratedIpc<RemoteUsageSourceSummary>({
    title: "更新远端用量来源失败",
    cmd: "remote_usage_custom_source_set_enabled",
    args: { id: normalizedId, enabled },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.remoteUsageCustomSourceSetEnabled({
          id: normalizedId,
          enabled,
        }),
        toSourceSummary
      ),
  });
}
