import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import {
  useProviderModelProbeMutation,
  useProviderModelsListMutation,
} from "../../query/providerModels";
import { useProvidersListQuery } from "../../query/providers";
import { ModelDetectionPage } from "../ModelDetectionPage";

vi.mock("sonner", () => ({ toast: vi.fn() }));
vi.mock("../../query/providers", () => ({ useProvidersListQuery: vi.fn() }));
vi.mock("../../query/providerModels", () => ({
  useProviderModelsListMutation: vi.fn(),
  useProviderModelProbeMutation: vi.fn(),
}));

const listModels = vi.fn();
const probeModel = vi.fn();

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe("pages/ModelDetectionPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useProvidersListQuery).mockReturnValue({
      data: [
        {
          id: 9,
          name: "Mixed Models",
          auth_mode: "api_key",
          base_urls: ["https://api.example.com/v1"],
        },
      ],
      isLoading: false,
    } as never);
    vi.mocked(useProviderModelsListMutation).mockReturnValue({
      isPending: false,
      mutateAsync: listModels,
    } as never);
    vi.mocked(useProviderModelProbeMutation).mockReturnValue({
      isPending: false,
      mutateAsync: probeModel,
    } as never);
    listModels.mockResolvedValue({
      ok: true,
      provider_id: 9,
      provider_name: "Mixed Models",
      cli_key: "codex",
      auth_mode: "api_key",
      base_url: "https://api.example.com/v1",
      endpoint: "https://api.example.com/v1/models",
      status: 200,
      latency_ms: 14,
      models: [
        {
          id: "deepseek-r1",
          display_name: "DeepSeek R1",
          owned_by: "deepseek",
          model_type: "model",
          supported_methods: [],
        },
        {
          id: "qwen3-235b",
          display_name: null,
          owned_by: "qwen",
          model_type: "model",
          supported_methods: [],
        },
      ],
      error: null,
      response_preview: null,
    });
    probeModel.mockResolvedValue({
      ok: true,
      provider_id: 9,
      provider_name: "Mixed Models",
      model: "deepseek-r1",
      protocol: "responses",
      endpoint: "https://api.example.com/v1/responses",
      status: 200,
      latency_ms: 21,
      outcome: "available",
      error: null,
      response_preview: null,
    });
  });

  it("fetches the directory without probing and probes only after the row button is clicked", async () => {
    render(<ModelDetectionPage />);

    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "9" } });
    fireEvent.click(screen.getByRole("button", { name: "获取模型列表" }));

    expect(await screen.findByText("deepseek-r1")).toBeInTheDocument();
    expect(screen.getByText("qwen3-235b")).toBeInTheDocument();
    expect(listModels).toHaveBeenCalledWith({
      providerId: 9,
      baseUrl: "https://api.example.com/v1",
    });
    expect(probeModel).not.toHaveBeenCalled();

    const detectButtons = screen.getAllByRole("button", { name: "检测" });
    fireEvent.click(detectButtons[0]);

    await waitFor(() => {
      expect(probeModel).toHaveBeenCalledWith({
        providerId: 9,
        model: "deepseek-r1",
        baseUrl: "https://api.example.com/v1",
      });
    });
    expect(probeModel).toHaveBeenCalledTimes(1);
    expect(screen.getByTitle("https://api.example.com/v1/responses")).toHaveTextContent(
      "可用· Responses"
    );
  });

  it("adds an unlisted model without triggering a probe", () => {
    render(<ModelDetectionPage />);

    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "9" } });
    fireEvent.change(screen.getByPlaceholderText("手动输入模型 ID"), {
      target: { value: "llama-4-local" },
    });
    fireEvent.click(screen.getByRole("button", { name: "添加模型" }));

    expect(screen.getByText("llama-4-local")).toBeInTheDocument();
    expect(screen.getByText("手动添加")).toBeInTheDocument();
    expect(listModels).not.toHaveBeenCalled();
    expect(probeModel).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  it("ignores an in-flight list result after the selected provider changes", async () => {
    const pendingList = deferred<Awaited<ReturnType<typeof listModels>>>();
    listModels.mockReturnValueOnce(pendingList.promise);
    vi.mocked(useProvidersListQuery).mockReturnValue({
      data: [
        {
          id: 9,
          name: "Mixed Models",
          auth_mode: "api_key",
          base_urls: ["https://api.example.com/v1"],
        },
        {
          id: 10,
          name: "Second Provider",
          auth_mode: "api_key",
          base_urls: ["https://second.example.com/v1"],
        },
      ],
      isLoading: false,
    } as never);

    render(<ModelDetectionPage />);
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "9" } });
    fireEvent.click(screen.getByRole("button", { name: "获取模型列表" }));
    fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "10" } });

    pendingList.resolve({
      ok: true,
      provider_id: 9,
      provider_name: "Mixed Models",
      cli_key: "codex",
      auth_mode: "api_key",
      base_url: "https://api.example.com/v1",
      endpoint: "https://api.example.com/v1/models",
      status: 200,
      latency_ms: 14,
      models: [
        {
          id: "stale-model",
          display_name: null,
          owned_by: null,
          model_type: null,
          supported_methods: [],
        },
      ],
      error: null,
      response_preview: null,
    });

    await waitFor(() => expect(listModels).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("stale-model")).not.toBeInTheDocument();
    expect(toast).not.toHaveBeenCalled();
  });
});
