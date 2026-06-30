import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "../../../generated/bindings";
import { monitoredServiceStatusModels, serviceStatusFetch } from "../serviceStatus";

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

  it("uses fixed monitored model order", () => {
    expect(monitoredServiceStatusModels()).toEqual(["gpt-5.5", "gpt-5.4", "gpt-5.4-mini"]);
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
});
