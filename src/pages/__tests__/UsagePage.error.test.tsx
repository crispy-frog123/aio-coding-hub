import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { ReactElement } from "react";
import { toast } from "sonner";
import { UsagePage } from "../UsagePage";
import { createTestQueryClient } from "../../test/utils/reactQuery";
import { setTauriRuntime } from "../../test/utils/tauriRuntime";
import { useCustomDateRange } from "../../hooks/useCustomDateRange";
import {
  useUsageLeaderboardV2Query,
  useUsageProviderCacheRateTrendV1Query,
  useUsageSummaryV2Query,
} from "../../query/usage";
import { useProvidersListQuery } from "../../query/providers";

vi.mock("sonner", () => ({ toast: vi.fn() }));

vi.mock("../../hooks/useCustomDateRange", async () => {
  const actual = await vi.importActual<typeof import("../../hooks/useCustomDateRange")>(
    "../../hooks/useCustomDateRange"
  );
  return { ...actual, useCustomDateRange: vi.fn() };
});

vi.mock("../../query/usage", async () => {
  const actual = await vi.importActual<typeof import("../../query/usage")>("../../query/usage");
  return {
    ...actual,
    useUsageSummaryV2Query: vi.fn(),
    useUsageLeaderboardV2Query: vi.fn(),
    useUsageProviderCacheRateTrendV1Query: vi.fn(),
  };
});

vi.mock("../../query/providers", async () => {
  const actual =
    await vi.importActual<typeof import("../../query/providers")>("../../query/providers");
  return { ...actual, useProvidersListQuery: vi.fn() };
});

function renderWithProviders(element: ReactElement) {
  const client = createTestQueryClient();
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>{element}</MemoryRouter>
    </QueryClientProvider>
  );
}

describe("pages/UsagePage (error)", () => {
  it("renders error card, toasts once, and allows retry", async () => {
    setTauriRuntime();
    vi.mocked(useProvidersListQuery).mockReturnValue({ data: [], isFetching: false } as any);

    vi.mocked(useCustomDateRange).mockReturnValue({
      customStartDate: "",
      setCustomStartDate: vi.fn(),
      customEndDate: "",
      setCustomEndDate: vi.fn(),
      customApplied: null,
      bounds: { startTs: 1, endTs: 2 },
      showCustomForm: false,
      applyCustomRange: vi.fn(),
      clearCustomRange: vi.fn(),
    } as any);

    const summaryRefetch = vi.fn().mockResolvedValue({ data: null });
    const leaderboardRefetch = vi.fn().mockResolvedValue({ data: [] });

    vi.mocked(useUsageSummaryV2Query).mockReturnValue({
      data: null,
      isFetching: false,
      error: new Error("boom"),
      refetch: summaryRefetch,
    } as any);
    vi.mocked(useUsageLeaderboardV2Query).mockReturnValue({
      data: [],
      isFetching: false,
      error: null,
      refetch: leaderboardRefetch,
    } as any);
    vi.mocked(useUsageProviderCacheRateTrendV1Query).mockReturnValue({
      data: [],
      isFetching: false,
      error: null,
      refetch: vi.fn(),
    } as any);

    renderWithProviders(<UsagePage />);

    fireEvent.click(screen.getByRole("tab", { name: "用量" }));
    await waitFor(() => {
      expect(toast).toHaveBeenCalledWith("加载用量失败：请重试（详情见页面错误信息）");
    });
    expect(screen.getByText("加载失败")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(summaryRefetch).toHaveBeenCalled();
    expect(leaderboardRefetch).toHaveBeenCalled();
  });
});
