import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { remoteUsageSnapshotsRefresh } from "../../services/usage/remoteUsage";
import { createQueryWrapper, createTestQueryClient } from "../../test/utils/reactQuery";
import { remoteUsageKeys } from "../keys";
import { useRemoteUsageSnapshotsQuery } from "../remoteUsage";

vi.mock("../../services/usage/remoteUsage", async () => {
  const actual = await vi.importActual<typeof import("../../services/usage/remoteUsage")>(
    "../../services/usage/remoteUsage"
  );
  return {
    ...actual,
    remoteUsageSnapshotsRefresh: vi.fn(),
  };
});

describe("query/remoteUsage", () => {
  it("query key includes cliKey and source ids", () => {
    expect(
      remoteUsageKeys.snapshots({
        cliKey: "codex",
        sourceIds: ["custom:2", "provider:1"],
      })
    ).toEqual(["remoteUsage", "snapshots", "codex", "custom:2", "provider:1"]);
  });

  it("calls snapshot refresh with normalized input", async () => {
    vi.mocked(remoteUsageSnapshotsRefresh).mockResolvedValue([]);
    const client = createTestQueryClient();
    const wrapper = createQueryWrapper(client);

    renderHook(
      () =>
        useRemoteUsageSnapshotsQuery({
          cliKey: " codex " as never,
          sourceIds: [" provider:1 "],
        }),
      { wrapper }
    );

    await waitFor(() => {
      expect(remoteUsageSnapshotsRefresh).toHaveBeenCalledWith({
        cliKey: "codex",
        sourceIds: ["provider:1"],
      });
    });
  });
});
