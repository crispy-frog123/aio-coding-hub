import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "../../../generated/bindings";
import {
  normalizeRemoteUsageRefreshInput,
  remoteUsageCustomSourceDelete,
  remoteUsageCustomSourceSetEnabled,
  remoteUsageCustomSourceUpsert,
  remoteUsageSnapshotsRefresh,
  remoteUsageSourcesList,
} from "../remoteUsage";

vi.mock("../../../generated/bindings", async () => {
  const actual = await vi.importActual<typeof import("../../../generated/bindings")>(
    "../../../generated/bindings"
  );
  return {
    ...actual,
    commands: {
      ...actual.commands,
      remoteUsageSourcesList: vi.fn(),
      remoteUsageSnapshotsRefresh: vi.fn(),
      remoteUsageCustomSourceUpsert: vi.fn(),
      remoteUsageCustomSourceDelete: vi.fn(),
      remoteUsageCustomSourceSetEnabled: vi.fn(),
    },
  };
});

describe("services/usage/remoteUsage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("normalizes refresh input and source ids", () => {
    expect(
      normalizeRemoteUsageRefreshInput({
        cliKey: " codex " as never,
        sourceIds: [" custom:2 ", "provider:1", "custom:2"],
      })
    ).toEqual({
      cliKey: "codex",
      sourceIds: ["custom:2", "provider:1"],
    });

    expect(() =>
      normalizeRemoteUsageRefreshInput({ cliKey: "codex", sourceIds: ["bad:1"] })
    ).toThrow("SEC_INVALID_INPUT");
    expect(normalizeRemoteUsageRefreshInput()).toEqual({ cliKey: null, sourceIds: null });
    expect(
      normalizeRemoteUsageRefreshInput({ cliKey: " " as never, sourceIds: [" ", ""] })
    ).toEqual({ cliKey: null, sourceIds: null });
    expect(() =>
      normalizeRemoteUsageRefreshInput({ cliKey: "unknown" as never, sourceIds: null })
    ).toThrow("SEC_INVALID_INPUT");
  });

  it("maps source rows and rejects leaked api keys", async () => {
    vi.mocked(commands.remoteUsageSourcesList).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          source_id: "provider:1",
          source_type: "provider",
          cli_key: "codex",
          name: "Provider",
          base_url: "https://example.com",
          endpoint_url: "https://example.com/v1/usage",
          enabled: true,
          provider_id: 1,
          custom_source_id: null,
          api_key_configured: true,
        },
      ],
    });

    const rows = await remoteUsageSourcesList(" codex " as never);

    expect(rows[0]?.cli_key).toBe("codex");
    expect(rows[0]?.source_type).toBe("provider");
    expect(commands.remoteUsageSourcesList).toHaveBeenCalledWith("codex");

    vi.mocked(commands.remoteUsageSourcesList).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          source_id: "custom:1",
          source_type: "custom",
          cli_key: "codex",
          name: "Leaky",
          base_url: "https://example.com",
          endpoint_url: "https://example.com/v1/usage",
          enabled: true,
          provider_id: null,
          custom_source_id: 1,
          api_key_configured: true,
          api_key: "sk-leaked",
        } as never,
      ],
    });

    await expect(remoteUsageSourcesList("codex")).rejects.toThrow("IPC_INVALID_RESULT");
  });

  it("maps snapshot rows without exposing api keys", async () => {
    vi.mocked(commands.remoteUsageSnapshotsRefresh).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          source: {
            source_id: "custom:1",
            source_type: "custom",
            cli_key: "codex",
            name: "Custom",
            base_url: "https://example.com",
            endpoint_url: "https://example.com/v1/usage",
            enabled: true,
            provider_id: null,
            custom_source_id: 1,
            api_key_configured: true,
          },
          status: "fresh",
          last_error: null,
          last_successful_refresh_at: 1,
          snapshot: {
            plan_name: "Pro",
            remaining: 2,
            unit: "USD",
            subscription: null,
            usage: { today: null, week: null, month: null, total: null },
            model_stats: [],
          },
        },
      ],
    });

    const rows = await remoteUsageSnapshotsRefresh({
      cliKey: "codex",
      sourceIds: ["custom:1"],
    });

    expect(rows[0]?.status).toBe("fresh");
    expect(rows[0]?.snapshot?.plan_name).toBe("Pro");
    expect(commands.remoteUsageSnapshotsRefresh).toHaveBeenCalledWith({
      cliKey: "codex",
      sourceIds: ["custom:1"],
    });
  });

  it("rejects malformed source and snapshot rows", async () => {
    vi.mocked(commands.remoteUsageSourcesList).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          source_id: "provider:1",
          source_type: "provider",
          cli_key: "codex",
          name: "Provider",
          base_url: "https://example.com",
          endpoint_url: "https://example.com/v1/usage",
          enabled: true,
          provider_id: 0,
          custom_source_id: null,
          api_key_configured: true,
        },
      ],
    });
    await expect(remoteUsageSourcesList("codex")).rejects.toThrow("IPC_INVALID_RESULT");

    vi.mocked(commands.remoteUsageSnapshotsRefresh).mockResolvedValueOnce({
      status: "ok",
      data: [
        {
          source: {
            source_id: "custom:1",
            source_type: "custom",
            cli_key: "codex",
            name: "Custom",
            base_url: "https://example.com",
            endpoint_url: "https://example.com/v1/usage",
            enabled: true,
            provider_id: null,
            custom_source_id: 1,
            api_key_configured: true,
          },
          status: "invalid" as never,
          last_error: null,
          last_successful_refresh_at: null,
          snapshot: null,
        },
      ],
    });
    await expect(remoteUsageSnapshotsRefresh()).rejects.toThrow("IPC_INVALID_LITERAL");
  });

  it("normalizes custom source writes and validates ids", async () => {
    const source = {
      source_id: "custom:2",
      source_type: "custom" as const,
      cli_key: "codex" as const,
      name: "Custom",
      base_url: "https://example.com",
      endpoint_url: "https://example.com/v1/usage",
      enabled: true,
      provider_id: null,
      custom_source_id: 2,
      api_key_configured: true,
    };

    vi.mocked(commands.remoteUsageCustomSourceUpsert).mockResolvedValueOnce({
      status: "ok",
      data: source,
    });
    await expect(
      remoteUsageCustomSourceUpsert({
        cliKey: " codex " as never,
        name: " Custom ",
        baseUrl: " https://example.com ",
        apiKey: " secret ",
        enabled: true,
      })
    ).resolves.toMatchObject({ source_id: "custom:2", custom_source_id: 2 });
    expect(commands.remoteUsageCustomSourceUpsert).toHaveBeenCalledWith({
      id: null,
      cliKey: "codex",
      name: "Custom",
      baseUrl: "https://example.com",
      apiKey: " secret ",
      enabled: true,
    });
    await expect(
      remoteUsageCustomSourceUpsert({
        cliKey: " " as never,
        name: "Custom",
        baseUrl: "https://example.com",
        enabled: true,
      })
    ).rejects.toThrow("cliKey is required");

    vi.mocked(commands.remoteUsageCustomSourceDelete).mockResolvedValueOnce({
      status: "ok",
      data: true,
    });
    await expect(remoteUsageCustomSourceDelete(2)).resolves.toBe(true);
    expect(commands.remoteUsageCustomSourceDelete).toHaveBeenCalledWith({ id: 2 });
    await expect(remoteUsageCustomSourceDelete(0)).rejects.toThrow("IPC_INVALID_RESULT");
    await expect(remoteUsageCustomSourceDelete(1.5)).rejects.toThrow("IPC_INVALID_RESULT");

    vi.mocked(commands.remoteUsageCustomSourceSetEnabled).mockResolvedValueOnce({
      status: "ok",
      data: { ...source, enabled: false },
    });
    await expect(remoteUsageCustomSourceSetEnabled(2, false)).resolves.toMatchObject({
      enabled: false,
    });
    expect(commands.remoteUsageCustomSourceSetEnabled).toHaveBeenCalledWith({
      id: 2,
      enabled: false,
    });
    await expect(remoteUsageCustomSourceSetEnabled(-1, true)).rejects.toThrow("IPC_INVALID_RESULT");
  });
});
