import { beforeEach, describe, expect, it, vi } from "vitest";
import { commands } from "../../../generated/bindings";
import { providerModelProbe, providerModelsList, validateProviderModelId } from "../providerModels";

vi.mock("../../../generated/bindings", async () => {
  const actual = await vi.importActual<typeof import("../../../generated/bindings")>(
    "../../../generated/bindings"
  );
  return {
    ...actual,
    commands: {
      ...actual.commands,
      providerModelsList: vi.fn(),
      providerModelProbe: vi.fn(),
    },
  };
});

describe("services/providers/providerModels", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("lists models with normalized provider and base URL arguments", async () => {
    const result = {
      ok: true,
      provider_id: 7,
      provider_name: "P1",
      cli_key: "codex",
      auth_mode: "api_key",
      base_url: "https://api.example.com/v1",
      endpoint: "https://api.example.com/v1/models",
      status: 200,
      latency_ms: 12,
      models: [],
      error: null,
      response_preview: null,
    };
    vi.mocked(commands.providerModelsList).mockResolvedValueOnce({ status: "ok", data: result });

    await expect(providerModelsList(7, " https://api.example.com/v1 ")).resolves.toEqual(result);
    expect(commands.providerModelsList).toHaveBeenCalledWith(7, "https://api.example.com/v1");
  });

  it("probes only the explicitly supplied model", async () => {
    const result = {
      ok: true,
      provider_id: 7,
      provider_name: "P1",
      model: "deepseek-r1",
      protocol: "responses",
      endpoint: "https://api.example.com/v1/responses",
      status: 200,
      latency_ms: 18,
      outcome: "available",
      error: null,
      response_preview: null,
    };
    vi.mocked(commands.providerModelProbe).mockResolvedValueOnce({ status: "ok", data: result });

    await expect(
      providerModelProbe({ providerId: 7, model: " deepseek-r1 ", baseUrl: null })
    ).resolves.toEqual(result);
    expect(commands.providerModelProbe).toHaveBeenCalledWith(7, "deepseek-r1", null);
  });

  it("rejects empty and oversized model IDs before IPC", async () => {
    expect(() => validateProviderModelId("  ")).toThrow("model is required");
    expect(() => validateProviderModelId("x".repeat(201))).toThrow("at most 200");
    await expect(providerModelProbe({ providerId: 7, model: "" })).rejects.toThrow(
      "model is required"
    );
    expect(commands.providerModelProbe).not.toHaveBeenCalled();
  });
});
