import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  codexReasoningAnalyticsAnalyze,
  codexReasoningAnalyticsBackfillFromRequestLogs,
  codexReasoningAnalyticsExport,
  codexReasoningAnalyticsImportJson,
  codexReasoningAnalyticsSnapshot,
} from "../../services/gateway/codexReasoningAnalytics";
import { createQueryWrapper, createTestQueryClient } from "../../test/utils/reactQuery";
import {
  useCodexReasoningAnalyticsAnalyzeQuery,
  useCodexReasoningAnalyticsBackfillMutation,
  useCodexReasoningAnalyticsExportMutation,
  useCodexReasoningAnalyticsImportJsonMutation,
  useCodexReasoningAnalyticsSnapshotQuery,
} from "../codexReasoningAnalytics";

vi.mock("../../services/gateway/codexReasoningAnalytics", () => ({
  codexReasoningAnalyticsAnalyze: vi.fn(),
  codexReasoningAnalyticsBackfillFromRequestLogs: vi.fn(),
  codexReasoningAnalyticsExport: vi.fn(),
  codexReasoningAnalyticsImportJson: vi.fn(),
  codexReasoningAnalyticsSnapshot: vi.fn(),
}));

describe("query/codexReasoningAnalytics", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("runs snapshot and analysis queries with default options", async () => {
    vi.mocked(codexReasoningAnalyticsSnapshot).mockResolvedValue({} as never);
    vi.mocked(codexReasoningAnalyticsAnalyze).mockResolvedValue({} as never);
    const wrapper = createQueryWrapper(createTestQueryClient());

    const snapshot = renderHook(() => useCodexReasoningAnalyticsSnapshotQuery({} as never), {
      wrapper,
    });
    const analysis = renderHook(() => useCodexReasoningAnalyticsAnalyzeQuery({} as never), {
      wrapper,
    });

    await waitFor(() => {
      expect(snapshot.result.current.isSuccess).toBe(true);
      expect(analysis.result.current.isSuccess).toBe(true);
    });
    expect(codexReasoningAnalyticsSnapshot).toHaveBeenCalledTimes(1);
    expect(codexReasoningAnalyticsAnalyze).toHaveBeenCalledTimes(1);
  });

  it("respects disabled query options with a custom interval", async () => {
    const wrapper = createQueryWrapper(createTestQueryClient());

    renderHook(
      () =>
        useCodexReasoningAnalyticsSnapshotQuery({} as never, {
          enabled: false,
          refetchIntervalMs: 60_000,
        }),
      { wrapper }
    );
    renderHook(
      () =>
        useCodexReasoningAnalyticsAnalyzeQuery({} as never, {
          enabled: false,
          refetchIntervalMs: 60_000,
        }),
      { wrapper }
    );
    await Promise.resolve();

    expect(codexReasoningAnalyticsSnapshot).not.toHaveBeenCalled();
    expect(codexReasoningAnalyticsAnalyze).not.toHaveBeenCalled();
  });

  it("runs mutations and invalidates analytics queries after writes", async () => {
    vi.mocked(codexReasoningAnalyticsBackfillFromRequestLogs).mockResolvedValue({} as never);
    vi.mocked(codexReasoningAnalyticsImportJson).mockResolvedValue({} as never);
    vi.mocked(codexReasoningAnalyticsExport).mockResolvedValue({} as never);
    const client = createTestQueryClient();
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");
    const wrapper = createQueryWrapper(client);

    const backfill = renderHook(() => useCodexReasoningAnalyticsBackfillMutation(), { wrapper });
    const importJson = renderHook(() => useCodexReasoningAnalyticsImportJsonMutation(), {
      wrapper,
    });
    const exportJson = renderHook(() => useCodexReasoningAnalyticsExportMutation(), { wrapper });

    await act(async () => {
      await backfill.result.current.mutateAsync({} as never);
      await importJson.result.current.mutateAsync({} as never);
      await exportJson.result.current.mutateAsync({} as never);
    });

    expect(codexReasoningAnalyticsBackfillFromRequestLogs).toHaveBeenCalledTimes(1);
    expect(codexReasoningAnalyticsImportJson).toHaveBeenCalledTimes(1);
    expect(codexReasoningAnalyticsExport).toHaveBeenCalledTimes(1);
    expect(invalidateQueries).toHaveBeenCalledTimes(2);
  });
});
