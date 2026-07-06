import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  codexReasoningAnalyticsAnalyze,
  codexReasoningAnalyticsBackfillFromRequestLogs,
  codexReasoningAnalyticsExport,
  codexReasoningAnalyticsImportJson,
  codexReasoningAnalyticsSnapshot,
  type CodexReasoningAnalyticsAnalyzeInput,
  type CodexReasoningAnalyticsBackfillInput,
  type CodexReasoningAnalyticsExportInput,
  type CodexReasoningAnalyticsImportJsonInput,
  type CodexReasoningAnalyticsSnapshotInput,
} from "../services/gateway/codexReasoningAnalytics";
import { codexReasoningAnalyticsKeys } from "./keys";

export function useCodexReasoningAnalyticsSnapshotQuery(
  input: CodexReasoningAnalyticsSnapshotInput,
  options?: { enabled?: boolean; refetchIntervalMs?: number | false }
) {
  return useQuery({
    queryKey: codexReasoningAnalyticsKeys.snapshot(input),
    queryFn: () => codexReasoningAnalyticsSnapshot(input),
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchIntervalMs ?? false,
  });
}

export function useCodexReasoningAnalyticsAnalyzeQuery(
  input: CodexReasoningAnalyticsAnalyzeInput,
  options?: { enabled?: boolean; refetchIntervalMs?: number | false }
) {
  return useQuery({
    queryKey: codexReasoningAnalyticsKeys.analyze(input),
    queryFn: () => codexReasoningAnalyticsAnalyze(input),
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchIntervalMs ?? false,
  });
}

export function useCodexReasoningAnalyticsBackfillMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: CodexReasoningAnalyticsBackfillInput) =>
      codexReasoningAnalyticsBackfillFromRequestLogs(input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: codexReasoningAnalyticsKeys.all });
    },
  });
}

export function useCodexReasoningAnalyticsImportJsonMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: CodexReasoningAnalyticsImportJsonInput) =>
      codexReasoningAnalyticsImportJson(input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: codexReasoningAnalyticsKeys.all });
    },
  });
}

export function useCodexReasoningAnalyticsExportMutation() {
  return useMutation({
    mutationFn: (input: CodexReasoningAnalyticsExportInput) => codexReasoningAnalyticsExport(input),
  });
}
