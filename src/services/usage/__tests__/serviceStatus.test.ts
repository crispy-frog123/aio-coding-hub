import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "../../../generated/bindings";
import { serviceStatusFetch } from "../serviceStatus";

vi.mock("../../../generated/bindings", async () => {
  const actual = await vi.importActual<typeof import("../../../generated/bindings")>(
    "../../../generated/bindings"
  );
  return {
    ...actual,
    commands: {
      ...actual.commands,
      serviceStatusFetch: vi.fn(),
    },
  };
});

describe("services/usage/serviceStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("fetches and validates status cell kinds", async () => {
    vi.mocked(commands.serviceStatusFetch).mockResolvedValueOnce({
      status: "ok",
      data: {
        error: null,
        snapshot: {
          endpoint_url: "https://status.input.im/api/status",
          refreshed_at: 1,
          raw_json_text: "{}",
          response: {
            all_ok: true,
            generated_at: 1,
            services: [
              {
                model: "gpt-5.5",
                uptime_pct: 99,
                latest_kind: "green",
                status_text: "正常",
                last: { ts: 1, ok: true, latency_ms: 100, error: null, kind: "green" },
                history: [{ ts: 1, ok: true, latency_ms: 100, error: null, kind: "green" }],
              },
            ],
          },
        },
      },
    });

    const result = await serviceStatusFetch();

    expect(result.snapshot?.response.services[0]?.latest_kind).toBe("green");
    expect(commands.serviceStatusFetch).toHaveBeenCalledTimes(1);
  });

  it("rejects invalid generated kind", async () => {
    vi.mocked(commands.serviceStatusFetch).mockResolvedValueOnce({
      status: "ok",
      data: {
        error: null,
        snapshot: {
          endpoint_url: "https://status.input.im/api/status",
          refreshed_at: 1,
          raw_json_text: "{}",
          response: {
            all_ok: true,
            generated_at: 1,
            services: [
              {
                model: "gpt-5.5",
                uptime_pct: 99,
                latest_kind: "blue",
                status_text: "??",
                last: null,
                history: [],
              } as never,
            ],
          },
        },
      },
    });

    await expect(serviceStatusFetch()).rejects.toThrow("IPC_INVALID_LITERAL");
  });

  it("preserves an empty snapshot", async () => {
    vi.mocked(commands.serviceStatusFetch).mockResolvedValueOnce({
      status: "ok",
      data: { error: "temporarily unavailable", snapshot: null },
    });

    await expect(serviceStatusFetch()).resolves.toEqual({
      error: "temporarily unavailable",
      snapshot: null,
    });
  });

  it("normalizes services without a latest probe", async () => {
    vi.mocked(commands.serviceStatusFetch).mockResolvedValueOnce({
      status: "ok",
      data: {
        error: null,
        snapshot: {
          endpoint_url: "https://status.input.im/api/status",
          refreshed_at: 1,
          raw_json_text: "{}",
          response: {
            all_ok: false,
            generated_at: 1,
            services: [
              {
                model: "gpt-5.4",
                uptime_pct: 95,
                latest_kind: "yellow",
                status_text: "波动",
                last: null,
                history: [],
              },
            ],
          },
        },
      },
    });

    const result = await serviceStatusFetch();
    expect(result.snapshot?.response.services[0]).toMatchObject({
      latest_kind: "yellow",
      last: null,
      history: [],
    });
  });
});
