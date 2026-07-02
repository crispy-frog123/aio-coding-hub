import {
  commands,
  type CodexReasoningAnalysisResult,
  type CodexReasoningAnalyticsAnalyzeInput,
  type CodexReasoningAnalyticsBackfillInput,
  type CodexReasoningAnalyticsBackfillReport,
  type CodexReasoningAnalyticsExport,
  type CodexReasoningAnalyticsExportInput,
  type CodexReasoningAnalyticsImportJsonInput,
  type CodexReasoningAnalyticsImportReport,
  type CodexReasoningAnalyticsSample,
  type CodexReasoningAnalyticsSnapshot,
  type CodexReasoningAnalyticsSnapshotInput,
} from "../../generated/bindings";
import { invokeGeneratedIpc, mapGeneratedCommandResponse } from "../generatedIpc";

export type {
  CodexReasoningAnalysisResult,
  CodexReasoningAnalyticsAnalyzeInput,
  CodexReasoningAnalyticsBackfillInput,
  CodexReasoningAnalyticsBackfillReport,
  CodexReasoningAnalyticsExport,
  CodexReasoningAnalyticsExportInput,
  CodexReasoningAnalyticsImportJsonInput,
  CodexReasoningAnalyticsImportReport,
  CodexReasoningAnalyticsSample,
  CodexReasoningAnalyticsSnapshot,
  CodexReasoningAnalyticsSnapshotInput,
};

export async function codexReasoningAnalyticsBackfillFromRequestLogs(
  input: CodexReasoningAnalyticsBackfillInput
) {
  return invokeGeneratedIpc<CodexReasoningAnalyticsBackfillReport>({
    title: "回填 Codex reasoning analytics 失败",
    cmd: "codex_reasoning_analytics_backfill_from_request_logs",
    args: { input },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.codexReasoningAnalyticsBackfillFromRequestLogs(input),
        (value) => value
      ),
  });
}

export async function codexReasoningAnalyticsSnapshot(
  input: CodexReasoningAnalyticsSnapshotInput
) {
  return invokeGeneratedIpc<CodexReasoningAnalyticsSnapshot>({
    title: "读取 Codex reasoning analytics 失败",
    cmd: "codex_reasoning_analytics_snapshot",
    args: { input },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.codexReasoningAnalyticsSnapshot(input),
        (value) => value
      ),
  });
}

export async function codexReasoningAnalyticsImportJson(
  input: CodexReasoningAnalyticsImportJsonInput
) {
  return invokeGeneratedIpc<CodexReasoningAnalyticsImportReport>({
    title: "导入 Codex reasoning analytics JSON 失败",
    cmd: "codex_reasoning_analytics_import_json",
    args: { input },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.codexReasoningAnalyticsImportJson(input),
        (value) => value
      ),
  });
}

export async function codexReasoningAnalyticsExport(
  input: CodexReasoningAnalyticsExportInput
) {
  return invokeGeneratedIpc<CodexReasoningAnalyticsExport>({
    title: "导出 Codex reasoning analytics 失败",
    cmd: "codex_reasoning_analytics_export",
    args: { input },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.codexReasoningAnalyticsExport(input),
        (value) => value
      ),
  });
}

export async function codexReasoningAnalyticsAnalyze(
  input: CodexReasoningAnalyticsAnalyzeInput
) {
  return invokeGeneratedIpc<CodexReasoningAnalysisResult>({
    title: "分析 Codex reasoning analytics 失败",
    cmd: "codex_reasoning_analytics_analyze",
    args: { input },
    invoke: async () =>
      mapGeneratedCommandResponse(
        await commands.codexReasoningAnalyticsAnalyze(input),
        (value) => value
      ),
  });
}
