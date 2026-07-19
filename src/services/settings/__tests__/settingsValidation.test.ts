import { describe, expect, it } from "vitest";
import {
  MAX_CODEX_PROVIDER_TEST_MODEL_NAME_LEN,
  MAX_CODEX_GATEWAY_TIMEOUT_MS,
  MAX_CODEX_REASONING_GUARD_MODEL_NAME_LEN,
  MAX_CODEX_REASONING_GUARD_MODEL_RULES_LEN,
  MAX_CODEX_REASONING_GUARD_REASONING_EQUALS_LEN,
  MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE,
  formatHostPort,
  parseCustomListenAddress,
  validateCodexProviderTestModel,
  validateCx2ccFallbackModel,
  validateCx2ccOptionalField,
  validateGatewayCustomListenAddress,
  validateSettingsSetInput,
  validateUpstreamProxyFields,
  validateWslCustomHostAddress,
} from "../settingsValidation";

describe("services/settings/settingsValidation", () => {
  it("accepts backend-aligned numeric boundary values", () => {
    expect(
      validateSettingsSetInput({
        preferredPort: 1024,
        logRetentionDays: 3650,
        providerCooldownSeconds: 0,
        providerBaseUrlPingCacheTtlSeconds: 1,
        upstreamFirstByteTimeoutSeconds: 3600,
        upstreamStreamIdleTimeoutSeconds: 0,
        upstreamRequestTimeoutNonStreamingSeconds: 86400,
        sseErrorRetryCount: 20,
        failoverMaxAttemptsPerProvider: 20,
        failoverMaxProvidersToTry: 5,
        circuitBreakerFailureThreshold: 50,
        circuitBreakerOpenDurationMinutes: 1440,
      })
    ).toBeNull();

    expect(validateSettingsSetInput({ upstreamStreamIdleTimeoutSeconds: 60 })).toBeNull();
  });

  it("rejects numeric settings outside backend bounds before IPC", () => {
    expect(validateSettingsSetInput({ preferredPort: 1023 })).toContain("首选端口必须 >= 1024");
    expect(validateSettingsSetInput({ logRetentionDays: 3651 })).toContain(
      "日志保留天数必须 <= 3650"
    );
    expect(validateSettingsSetInput({ providerCooldownSeconds: 3601 })).toContain(
      "Provider 冷却时间必须 <= 3600"
    );
    expect(validateSettingsSetInput({ providerBaseUrlPingCacheTtlSeconds: 0 })).toContain(
      "Provider Base URL 探测缓存 TTL必须 >= 1"
    );
    expect(validateSettingsSetInput({ upstreamFirstByteTimeoutSeconds: 3601 })).toContain(
      "首字节超时必须 <= 3600"
    );
    expect(
      validateSettingsSetInput({ upstreamRequestTimeoutNonStreamingSeconds: 86401 })
    ).toContain("非流式请求超时必须 <= 86400");
    expect(validateSettingsSetInput({ sseErrorRetryCount: 21 })).toContain(
      "SSE 错误重试次数必须 <= 20"
    );
    expect(validateSettingsSetInput({ circuitBreakerFailureThreshold: 0 })).toContain(
      "熔断失败阈值必须 >= 1"
    );
    expect(validateSettingsSetInput({ circuitBreakerOpenDurationMinutes: 1441 })).toContain(
      "熔断打开时长必须 <= 1440"
    );
  });

  it("rejects fractional values and stream idle timeout values in the forbidden gap", () => {
    expect(validateSettingsSetInput({ preferredPort: 37123.5 })).toContain("首选端口必须是整数");
    expect(validateSettingsSetInput({ upstreamStreamIdleTimeoutSeconds: 30 })).toContain(
      "流式空闲超时必须为 0"
    );
    expect(validateSettingsSetInput({ upstreamStreamIdleTimeoutSeconds: 3601 })).toContain(
      "流式空闲超时必须 <= 3600"
    );
  });

  it("rejects failover product overflow when both dimensions are present", () => {
    expect(
      validateSettingsSetInput({
        failoverMaxAttemptsPerProvider: 20,
        failoverMaxProvidersToTry: 6,
      })
    ).toContain("Failover 总尝试次数必须 <= 100");
  });

  it("parses and formats custom gateway listen addresses", () => {
    expect(parseCustomListenAddress("")).toEqual({ host: "0.0.0.0", port: null });
    expect(parseCustomListenAddress("127.0.0.1:37123")).toEqual({
      host: "127.0.0.1",
      port: 37123,
    });
    expect(parseCustomListenAddress("[::1]:37123")).toEqual({ host: "::1", port: 37123 });
    expect(parseCustomListenAddress("https://127.0.0.1:37123")).toBeNull();
    expect(parseCustomListenAddress("127.0.0.1:80")).toBeNull();

    expect(formatHostPort("127.0.0.1", 37123)).toBe("127.0.0.1:37123");
    expect(formatHostPort("::1", 37123)).toBe("[::1]:37123");
    expect(validateGatewayCustomListenAddress("127.0.0.1:abc")).toContain("自定义地址仅支持");
    expect(validateGatewayCustomListenAddress("127.0.0.1:80")).toContain("端口必须 >= 1024");
  });

  it("validates WSL custom host values without accepting URLs or ports", () => {
    expect(validateWslCustomHostAddress("host.docker.internal")).toBeNull();
    expect(validateWslCustomHostAddress("[::1]")).toBeNull();
    expect(validateWslCustomHostAddress("::1")).toBeNull();
    expect(validateWslCustomHostAddress("http://localhost")).toContain("不要包含协议或路径");
    expect(validateWslCustomHostAddress("[::1")).toContain("缺少右方括号");
    expect(validateWslCustomHostAddress("[::1]:37123")).toContain("不要包含端口");
    expect(validateWslCustomHostAddress("127.0.0.1:37123")).toContain("不支持端口");
  });

  it("validates update URLs, proxy credentials, and CX2CC text fields", () => {
    expect(
      validateSettingsSetInput({
        updateReleasesUrl: "ftp://example.com/releases.json",
      })
    ).toContain("更新地址仅支持 http 或 https");
    expect(validateSettingsSetInput({ updateReleasesUrl: "https://u:p@example.com" })).toContain(
      "更新地址不能包含用户名或密码"
    );
    expect(
      validateSettingsSetInput({ updateReleasesUrl: "https://example.com/releases" })
    ).toBeNull();

    expect(validateUpstreamProxyFields({ enabled: true, url: "" })).toContain("代理地址不能为空");
    expect(
      validateUpstreamProxyFields({ url: "not a url", validateUrlWhenPresent: true })
    ).toContain("代理地址不是有效 URL");
    expect(
      validateUpstreamProxyFields({ url: "ftp://example.com", validateUrlWhenPresent: true })
    ).toContain("代理地址协议仅支持");
    expect(
      validateUpstreamProxyFields({
        url: "https://user:pass@example.com",
        username: "user",
        validateUrlWhenPresent: true,
      })
    ).toContain("代理认证信息不要同时写在 URL");
    expect(
      validateUpstreamProxyFields({ url: "https://example.com", username: "user" })
    ).toBeNull();
    expect(
      validateUpstreamProxyFields({
        url: "https://example.com",
        passwordUpdate: { mode: "replace", value: "secret" },
      })
    ).toContain("填写代理密码时也需要填写用户名");

    expect(validateCx2ccFallbackModel("模型", " claude-3 ")).toBeNull();
    expect(validateCx2ccFallbackModel("模型", "")).toContain("模型不能为空");
    expect(validateCx2ccFallbackModel("模型", "bad\u0000name")).toContain("模型不能包含控制字符");
    expect(validateCx2ccOptionalField("推理强度", "")).toBeNull();
    expect(validateCx2ccOptionalField("推理强度", "x".repeat(65))).toContain("推理强度必须 <=");
  });

  it("parses host-only and IPv6 listen addresses with strict formats", () => {
    expect(parseCustomListenAddress("localhost")).toEqual({ host: "localhost", port: null });
    expect(parseCustomListenAddress("[::1]")).toEqual({ host: "::1", port: null });
    expect(parseCustomListenAddress("[::1")).toBeNull();
    expect(parseCustomListenAddress("[]")).toBeNull();
    expect(parseCustomListenAddress("[::1]8080")).toBeNull();
    expect(parseCustomListenAddress("[::1]:abc")).toBeNull();
    expect(parseCustomListenAddress(":8080")).toBeNull();
    expect(parseCustomListenAddress("1:2:3")).toBeNull();
    expect(parseCustomListenAddress("127.0.0.1:70000")).toBeNull();
    expect(parseCustomListenAddress(`127.0.0.1:${"9".repeat(400)}`)).toBeNull();
    expect(validateGatewayCustomListenAddress("[::1]:80")).toContain("端口必须 >= 1024");
  });

  it("validates WSL host edge cases: blank input and malformed brackets", () => {
    expect(validateWslCustomHostAddress("  ")).toBeNull();
    expect(validateWslCustomHostAddress("[]")).toContain("IPv6 宿主机地址请使用");
    expect(validateWslCustomHostAddress("::1]")).toContain("IPv6 宿主机地址请使用");
    expect(validateWslCustomHostAddress("a[b")).toContain("IPv6 宿主机地址请使用");
  });

  it("validates update URL emptiness, length, and format", () => {
    expect(validateSettingsSetInput({ updateReleasesUrl: "  " })).toBeNull();
    expect(
      validateSettingsSetInput({ updateReleasesUrl: `https://example.com/${"a".repeat(2050)}` })
    ).toContain("更新地址必须 <= 2048");
    expect(validateSettingsSetInput({ updateReleasesUrl: "not a url" })).toContain(
      "更新地址不是有效 URL"
    );
  });

  it("rejects oversized proxy URL and password", () => {
    expect(
      validateUpstreamProxyFields({ url: `http://example.com/${"a".repeat(2050)}` })
    ).toContain("代理地址必须 <= 2048");
    expect(validateUpstreamProxyFields({ username: "u", password: "a".repeat(4097) })).toContain(
      "代理密码必须 <= 4096"
    );
  });

  it("covers stream idle integer check, valid custom WSL host, and CX2CC branches", () => {
    expect(validateSettingsSetInput({ upstreamStreamIdleTimeoutSeconds: 30.5 })).toContain(
      "流式空闲超时必须是整数"
    );
    expect(
      validateSettingsSetInput({
        wslHostAddressMode: "custom",
        wslCustomHostAddress: "host.docker.internal",
      })
    ).toBeNull();
    expect(validateSettingsSetInput({ cx2CcFallbackModelOpus: "" })).toContain(
      "CX2CC Opus 默认模型不能为空"
    );
    expect(validateSettingsSetInput({ cx2CcServiceTier: "x".repeat(65) })).toContain(
      "CX2CC 服务层级必须 <="
    );
    expect(
      validateSettingsSetInput({
        cx2CcFallbackModelSonnet: "claude-sonnet-4",
        cx2CcModelReasoningEffort: "high",
        cx2CcServiceTier: "flex",
      })
    ).toBeNull();
  });

  it("runs composite settings validation only for enabled custom modes", () => {
    expect(
      validateSettingsSetInput({
        gatewayListenMode: "custom",
        gatewayCustomListenAddress: "127.0.0.1:80",
      })
    ).toContain("端口必须 >= 1024");
    expect(
      validateSettingsSetInput({
        gatewayListenMode: "localhost",
        gatewayCustomListenAddress: "127.0.0.1:80",
      })
    ).toBeNull();
    expect(
      validateSettingsSetInput({
        wslHostAddressMode: "custom",
        wslCustomHostAddress: "127.0.0.1:37123",
      })
    ).toContain("不支持端口");
    expect(
      validateSettingsSetInput({
        wslHostAddressMode: "auto",
        wslCustomHostAddress: "127.0.0.1:37123",
      })
    ).toBeNull();
  });

  it("validates the Codex provider test model", () => {
    expect(validateCodexProviderTestModel("测试模型", " gpt-5.4 ")).toBeNull();
    expect(validateCodexProviderTestModel("测试模型", "  ")).toContain("不能为空");
    expect(
      validateCodexProviderTestModel(
        "测试模型",
        "x".repeat(MAX_CODEX_PROVIDER_TEST_MODEL_NAME_LEN + 1)
      )
    ).toContain(`必须 <= ${MAX_CODEX_PROVIDER_TEST_MODEL_NAME_LEN}`);
    expect(validateCodexProviderTestModel("测试模型", "bad\u0000model")).toContain(
      "不能包含控制字符"
    );
    expect(validateSettingsSetInput({ codexProviderTestModel: "gpt-5.4" })).toBeNull();
    expect(validateSettingsSetInput({ codexProviderTestModel: "" })).toContain("不能为空");
  });

  it("accepts supported Codex reasoning guard modes and rejects unknown modes", () => {
    const accepted = [
      { codexReasoningGuardCompareMode: "equals" },
      { codexReasoningGuardCompareMode: "less_than_or_equal" },
      { codexReasoningGuardRuleMode: "reasoning_tokens" },
      { codexReasoningGuardRuleMode: "final_answer_only_high_xhigh" },
      { codexReasoningGuardMatchMode: "manual" },
      { codexReasoningGuardMatchMode: "formula_518n_minus_2" },
      { codexReasoningGuardMatchMode: "formula518n_minus2" },
      { codexReasoningGuardMatchMode: "formula_51_8n_minus_2" },
      { codexReasoningGuardStreamAction: "strict_502" },
      { codexReasoningGuardStreamAction: "disconnect" },
      { codexReasoningGuardStreamAction: "continuation_recovery" },
      { codexReasoningGuardExhaustedAction: "return_error" },
      { codexReasoningGuardExhaustedAction: "switch_provider" },
    ];

    for (const input of accepted) {
      expect(validateSettingsSetInput(input as never)).toBeNull();
    }

    const rejected = [
      [{ codexReasoningGuardCompareMode: "invalid" }, "比较模式"],
      [{ codexReasoningGuardRuleMode: "invalid" }, "规则模式"],
      [{ codexReasoningGuardMatchMode: "invalid" }, "命中来源"],
      [{ codexReasoningGuardStreamAction: "invalid" }, "流式命中动作"],
      [{ codexReasoningGuardExhaustedAction: "invalid" }, "预算耗尽动作"],
    ] as const;

    for (const [input, message] of rejected) {
      expect(validateSettingsSetInput(input as never)).toContain(message);
    }
  });

  it("validates Codex reasoning token lists and continuation markers", () => {
    expect(
      validateSettingsSetInput({ codexReasoningGuardReasoningEquals: "invalid" as never })
    ).toContain("至少需要一个");
    expect(validateSettingsSetInput({ codexReasoningGuardReasoningEquals: [] })).toContain(
      "至少需要一个"
    );
    expect(
      validateSettingsSetInput({
        codexReasoningGuardReasoningEquals: Array.from(
          { length: MAX_CODEX_REASONING_GUARD_REASONING_EQUALS_LEN + 1 },
          (_, index) => index
        ),
      })
    ).toContain("最多支持");
    expect(validateSettingsSetInput({ codexReasoningGuardReasoningEquals: [1.5] })).toContain(
      "必须是整数列表"
    );
    expect(validateSettingsSetInput({ codexReasoningGuardReasoningEquals: [-1] })).toContain(
      "必须在 0 到"
    );
    expect(
      validateSettingsSetInput({
        codexReasoningGuardReasoningEquals: [MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE + 1],
      })
    ).toContain("必须在 0 到");
    expect(
      validateSettingsSetInput({ codexReasoningGuardReasoningEquals: [0, 516, 1034] })
    ).toBeNull();

    expect(
      validateSettingsSetInput({ codexReasoningGuardContinuationMarkerText: " Continue " })
    ).toBeNull();
    expect(validateSettingsSetInput({ codexReasoningGuardContinuationMarkerText: " " })).toContain(
      "marker 不能为空"
    );
    expect(
      validateSettingsSetInput({ codexReasoningGuardContinuationMarkerText: "x".repeat(257) })
    ).toContain("marker 必须 <= 256");
    expect(
      validateSettingsSetInput({ codexReasoningGuardContinuationMarkerText: "bad\u0000marker" })
    ).toContain("不能包含控制字符");
  });

  it("validates Codex reasoning guard retry budgets", () => {
    expect(
      validateSettingsSetInput({
        codexReasoningGuardImmediateRetryBudget: 100,
        codexReasoningGuardDelayedRetryBudget: 100,
        codexReasoningGuardDelayedRetryMs: 60_000,
        codexReasoningGuardBackoffAfterHits: 100,
        codexReasoningGuardBackoffMs: 60_000,
      })
    ).toBeNull();
    expect(validateSettingsSetInput({ codexReasoningGuardImmediateRetryBudget: 1.5 })).toContain(
      "必须是整数"
    );
    expect(validateSettingsSetInput({ codexReasoningGuardDelayedRetryBudget: -1 })).toContain(
      "必须 >= 0"
    );
    expect(validateSettingsSetInput({ codexReasoningGuardDelayedRetryMs: 60_001 })).toContain(
      "必须 <= 60000"
    );
    expect(validateSettingsSetInput({ codexReasoningGuardBackoffAfterHits: 101 })).toContain(
      "必须 <= 100"
    );
    expect(validateSettingsSetInput({ codexReasoningGuardBackoffMs: 60_001 })).toContain(
      "必须 <= 60000"
    );
  });

  it("validates layered Codex gateway policy actions and timeouts", () => {
    for (const action of [
      "pass_through",
      "return_502",
      "retry_then_pass_through",
      "retry_then_502",
    ] as const) {
      expect(validateSettingsSetInput({ codexGatewayCapacityErrorAction: action })).toBeNull();
      expect(validateSettingsSetInput({ codexGatewayHttp429Action: action })).toBeNull();
    }
    expect(
      validateSettingsSetInput({ codexGatewayCapacityErrorAction: "invalid" as never })
    ).toContain("Capacity 策略");
    expect(validateSettingsSetInput({ codexGatewayHttp429Action: "invalid" as never })).toContain(
      "HTTP 429 策略"
    );
    expect(
      validateSettingsSetInput({
        codexGatewayFirstProgressAction: "retry_then_pass_through" as never,
      })
    ).toContain("首个有效输出超时动作");
    expect(
      validateSettingsSetInput({
        codexGatewayLatencyGuardEnabled: true,
        codexGatewayFirstProgressTimeoutMs: 0,
        codexGatewayTotalTimeoutMs: 0,
      })
    ).toContain("至少填写一个非零值");
    expect(
      validateSettingsSetInput({
        codexGatewayLatencyGuardEnabled: true,
        codexGatewayFirstProgressTimeoutMs: 15_000,
        codexGatewayFirstProgressAction: "retry_then_502",
        codexGatewayTotalTimeoutMs: 120_000,
      })
    ).toBeNull();
    expect(
      validateSettingsSetInput({ codexGatewayTotalTimeoutMs: MAX_CODEX_GATEWAY_TIMEOUT_MS + 1 })
    ).toContain(`必须 <= ${MAX_CODEX_GATEWAY_TIMEOUT_MS}`);
    expect(validateSettingsSetInput({ codexGatewayLatencyGuardEnabled: true })).toBeNull();
  });

  it("validates per-model Codex reasoning guard rules", () => {
    const validRule = {
      requested_model: "gpt-5.4",
      compare_mode: "equals" as const,
      reasoning_equals: [516],
    };

    expect(
      validateSettingsSetInput({ codexReasoningGuardModelRules: "invalid" as never })
    ).toContain("必须是列表");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: Array.from(
          { length: MAX_CODEX_REASONING_GUARD_MODEL_RULES_LEN + 1 },
          (_, index) => ({ ...validRule, requested_model: `gpt-${index}` })
        ),
      })
    ).toContain("最多支持");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, requested_model: " " }],
      })
    ).toContain("必须填写模型名");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [
          {
            ...validRule,
            requested_model: "x".repeat(MAX_CODEX_REASONING_GUARD_MODEL_NAME_LEN + 1),
          },
        ],
      })
    ).toContain("模型名必须 <=");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, requested_model: "bad\u0000model" }],
      })
    ).toContain("模型名不能包含控制字符");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [validRule, { ...validRule, requested_model: "GPT-5.4" }],
      })
    ).toContain("重复");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, compare_mode: "invalid" as never }],
      })
    ).toContain("比较模式");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, reasoning_equals: [] }],
      })
    ).toContain("至少需要一个");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [
          {
            ...validRule,
            reasoning_equals: Array.from(
              { length: MAX_CODEX_REASONING_GUARD_REASONING_EQUALS_LEN + 1 },
              (_, index) => index
            ),
          },
        ],
      })
    ).toContain("最多支持");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, reasoning_equals: [1.5] }],
      })
    ).toContain("必须是整数列表");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [{ ...validRule, reasoning_equals: [-1] }],
      })
    ).toContain("值必须在 0 到");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [
          {
            ...validRule,
            reasoning_equals: [MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE + 1],
          },
        ],
      })
    ).toContain("值必须在 0 到");
    expect(
      validateSettingsSetInput({
        codexReasoningGuardModelRules: [
          validRule,
          {
            requested_model: "gpt-5.5",
            compare_mode: "less_than_or_equal",
            reasoning_equals: [0, MAX_CODEX_REASONING_GUARD_REASONING_TOKEN_VALUE],
          },
        ],
      })
    ).toBeNull();
  });
});
