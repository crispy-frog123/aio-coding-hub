import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { settingsGet, settingsSet } from "../../services/settings/settings";
import { createTestAppSettings } from "../../test/fixtures/settings";
import { ReasoningGuardPage } from "../ReasoningGuardPage";
import { createTestQueryClient } from "../../test/utils/reactQuery";
import { server } from "../../test/msw/server";
import { TAURI_ENDPOINT } from "../../test/tauriEndpoint";

vi.mock("sonner", () => ({ toast: vi.fn() }));
vi.mock("../../services/settings/settings", async () => {
  const actual = await vi.importActual<typeof import("../../services/settings/settings")>(
    "../../services/settings/settings"
  );
  return {
    ...actual,
    settingsGet: vi.fn(),
    settingsSet: vi.fn(),
  };
});

const STATS = {
  checked_request_count: 0,
  checked_response_count: 0,
  hit_request_count: 0,
  hit_attempt_count: 0,
  normal_request_count: 0,
  total_request_count: 0,
  hit_rate: 0,
  by_model: [],
};

function renderPage() {
  const client = createTestQueryClient();
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ReasoningGuardPage />
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe("ReasoningGuardPage", () => {
  beforeEach(() => {
    const settings = createTestAppSettings();
    vi.mocked(settingsGet).mockResolvedValue(settings);
    vi.mocked(settingsSet).mockImplementation(async (input) => ({
      settings: {
        ...settings,
        codex_gateway_latency_guard_enabled:
          input.codexGatewayLatencyGuardEnabled ?? settings.codex_gateway_latency_guard_enabled,
        codex_gateway_first_progress_timeout_ms:
          input.codexGatewayFirstProgressTimeoutMs ??
          settings.codex_gateway_first_progress_timeout_ms,
        codex_gateway_total_timeout_ms:
          input.codexGatewayTotalTimeoutMs ?? settings.codex_gateway_total_timeout_ms,
      },
      runtime: {
        gateway_rebound: false,
        cli_proxy_synced: false,
        wsl_auto_sync_triggered: false,
        gateway_status: {
          running: false,
          port: null,
          base_url: null,
          listen_addr: null,
        },
      },
    }));
    server.use(
      http.post(`${TAURI_ENDPOINT}/request_logs_codex_reasoning_guard_stats`, () =>
        HttpResponse.json(STATS)
      )
    );
  });

  it("seeds a usable first-progress timeout when enabling an all-zero guard", async () => {
    renderPage();

    const section = await screen.findByRole("heading", { name: "响应超时保护" });
    const card = section.closest("div.border-t") as HTMLElement;
    const toggle = within(card).getByRole("switch");
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);

    expect(within(card).getByDisplayValue("300000")).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    await waitFor(() => {
      expect(settingsSet).toHaveBeenCalledWith(
        expect.objectContaining({
          codexGatewayLatencyGuardEnabled: true,
          codexGatewayFirstProgressTimeoutMs: 300_000,
          codexGatewayTotalTimeoutMs: 0,
        })
      );
    });
    expect(toast).toHaveBeenCalledWith("降智拦截设置已保存");
  });

  it("shows an immediately visible error when enabled thresholds are both zero", async () => {
    renderPage();

    const section = await screen.findByRole("heading", { name: "响应超时保护" });
    const card = section.closest("div.border-t") as HTMLElement;
    const toggle = within(card).getByRole("switch");
    await waitFor(() => expect(toggle).toBeEnabled());
    fireEvent.click(toggle);
    fireEvent.change(within(card).getByDisplayValue("300000"), { target: { value: "0" } });
    fireEvent.click(screen.getByRole("button", { name: "保存设置" }));

    const message = "启用响应超时保护时，首个有效输出或请求总 deadline 至少填写一个非零值";
    expect(await within(card).findByRole("alert")).toHaveTextContent(message);
    expect(toast).toHaveBeenCalledWith(message);
    expect(settingsSet).not.toHaveBeenCalled();
  });
});
