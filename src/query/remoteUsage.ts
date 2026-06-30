// Usage: React Query hooks for remote sub2api usage snapshots.

import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { CliKey } from "../services/providers/providers";
import {
  normalizeRemoteUsageRefreshInput,
  remoteUsageCustomSourceDelete,
  remoteUsageCustomSourceSetEnabled,
  remoteUsageCustomSourceUpsert,
  remoteUsageSnapshotsRefresh,
  remoteUsageSourcesList,
  type RemoteUsageCustomSourceUpsertInput,
  type RemoteUsageRefreshInput,
} from "../services/usage/remoteUsage";
import { remoteUsageKeys } from "./keys";

type RemoteUsageQueryOptions = {
  enabled?: boolean;
  refetchIntervalMs?: number | false;
};

export function useRemoteUsageSourcesQuery(
  cliKey: CliKey | null,
  options?: RemoteUsageQueryOptions
) {
  return useQuery({
    queryKey: remoteUsageKeys.sources(cliKey),
    queryFn: () => remoteUsageSourcesList(cliKey),
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchIntervalMs ?? false,
  });
}

export function useRemoteUsageSnapshotsQuery(
  input: RemoteUsageRefreshInput,
  options?: RemoteUsageQueryOptions
) {
  const normalized = normalizeRemoteUsageRefreshInput(input);
  return useQuery({
    queryKey: remoteUsageKeys.snapshots(normalized),
    queryFn: () => remoteUsageSnapshotsRefresh(normalized),
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchIntervalMs ?? false,
  });
}

export function useRemoteUsageCustomSourceUpsertMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: RemoteUsageCustomSourceUpsertInput) => remoteUsageCustomSourceUpsert(input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: remoteUsageKeys.all });
    },
  });
}

export function useRemoteUsageCustomSourceDeleteMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => remoteUsageCustomSourceDelete(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: remoteUsageKeys.all });
    },
  });
}

export function useRemoteUsageCustomSourceEnabledMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      remoteUsageCustomSourceSetEnabled(id, enabled),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: remoteUsageKeys.all });
    },
  });
}
