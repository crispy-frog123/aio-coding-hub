import { renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { serviceStatusFetch } from "../../services/usage/serviceStatus";
import { createQueryWrapper, createTestQueryClient } from "../../test/utils/reactQuery";
import { useServiceStatusQuery } from "../serviceStatus";

vi.mock("../../services/usage/serviceStatus", () => ({
  serviceStatusFetch: vi.fn(),
}));

describe("query/serviceStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("fetches status with default options", async () => {
    vi.mocked(serviceStatusFetch).mockResolvedValue({ error: null, snapshot: null });
    const wrapper = createQueryWrapper(createTestQueryClient());

    const { result } = renderHook(() => useServiceStatusQuery(), { wrapper });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(serviceStatusFetch).toHaveBeenCalledTimes(1);
  });

  it("respects disabled and custom interval options", async () => {
    const wrapper = createQueryWrapper(createTestQueryClient());

    renderHook(() => useServiceStatusQuery({ enabled: false, refetchIntervalMs: 60_000 }), {
      wrapper,
    });
    await Promise.resolve();

    expect(serviceStatusFetch).not.toHaveBeenCalled();
  });
});
