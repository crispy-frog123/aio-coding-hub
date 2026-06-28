import type { UpstreamRetryPolicy, UpstreamTransportRetryKind } from "../settings/settings";

export const DEFAULT_UPSTREAM_RETRY_POLICY: UpstreamRetryPolicy = {
  enabled: true,
  status_codes: [502, 503, 504],
  transport_errors: ["connect", "timeout", "read"],
  max_retries: 1,
  backoff_ms: 100,
  counts_toward_circuit_breaker: false,
};

export const UPSTREAM_RETRY_STATUS_CODES = [502, 503, 504] as const;
export const UPSTREAM_RETRY_TRANSPORT_ERRORS = [
  "connect",
  "timeout",
  "read",
] as const satisfies readonly UpstreamTransportRetryKind[];

export const UPSTREAM_RETRY_TRANSPORT_ERROR_LABELS: Record<UpstreamTransportRetryKind, string> = {
  connect: "连接失败",
  timeout: "超时",
  read: "读取失败",
};

export function cloneUpstreamRetryPolicy(
  policy: UpstreamRetryPolicy | null | undefined
): UpstreamRetryPolicy {
  return {
    ...(policy ?? DEFAULT_UPSTREAM_RETRY_POLICY),
    status_codes: [...(policy?.status_codes ?? DEFAULT_UPSTREAM_RETRY_POLICY.status_codes)],
    transport_errors: [
      ...(policy?.transport_errors ?? DEFAULT_UPSTREAM_RETRY_POLICY.transport_errors),
    ],
  };
}

export function toggleRetryStatusCode(policy: UpstreamRetryPolicy, statusCode: number) {
  const selected = new Set(policy.status_codes);
  if (selected.has(statusCode)) {
    selected.delete(statusCode);
  } else {
    selected.add(statusCode);
  }
  return { ...policy, status_codes: Array.from(selected).sort((a, b) => a - b) };
}

export function toggleRetryTransportError(
  policy: UpstreamRetryPolicy,
  kind: UpstreamTransportRetryKind
) {
  const selected = new Set(policy.transport_errors);
  if (selected.has(kind)) {
    selected.delete(kind);
  } else {
    selected.add(kind);
  }
  return {
    ...policy,
    transport_errors: UPSTREAM_RETRY_TRANSPORT_ERRORS.filter((item) => selected.has(item)),
  };
}

export function validateUpstreamRetryPolicy(policy: UpstreamRetryPolicy) {
  if (!Number.isInteger(policy.max_retries) || policy.max_retries < 0 || policy.max_retries > 10) {
    return "瞬时错误重试次数必须为 0-10";
  }
  if (!Number.isInteger(policy.backoff_ms) || policy.backoff_ms < 0 || policy.backoff_ms > 60000) {
    return "重试间隔必须为 0-60000 毫秒";
  }
  if (policy.enabled && policy.status_codes.length === 0 && policy.transport_errors.length === 0) {
    return "启用重试时至少选择一个 HTTP 状态码或传输错误";
  }
  return null;
}
