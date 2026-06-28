import type {
  UpstreamRetryPolicy,
  UpstreamTransportRetryKind,
} from "../../services/settings/settings";
import {
  toggleRetryStatusCode,
  toggleRetryTransportError,
  UPSTREAM_RETRY_STATUS_CODES,
  UPSTREAM_RETRY_TRANSPORT_ERROR_LABELS,
  UPSTREAM_RETRY_TRANSPORT_ERRORS,
} from "../../services/gateway/upstreamRetryPolicy";
import { FormField } from "../../ui/FormField";
import { Input } from "../../ui/Input";
import { Switch } from "../../ui/Switch";

export function RetryPolicyFields({
  policy,
  disabled,
  onChange,
}: {
  policy: UpstreamRetryPolicy;
  disabled: boolean;
  onChange: (policy: UpstreamRetryPolicy) => void;
}) {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <div className="text-sm font-medium text-foreground">启用瞬时错误重试</div>
          <div className="text-xs text-muted-foreground">
            关闭后匹配错误也会直接进入切换/失败流程。
          </div>
        </div>
        <Switch
          checked={policy.enabled}
          onCheckedChange={(checked) => onChange({ ...policy, enabled: checked })}
          disabled={disabled}
        />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <div className="space-y-2">
          <div className="text-xs font-medium text-muted-foreground">HTTP 状态码</div>
          <div className="flex flex-wrap gap-2">
            {UPSTREAM_RETRY_STATUS_CODES.map((statusCode) => (
              <label
                key={statusCode}
                className="inline-flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5 text-xs text-secondary-foreground"
              >
                <input
                  type="checkbox"
                  checked={policy.status_codes.includes(statusCode)}
                  disabled={disabled}
                  onChange={() => onChange(toggleRetryStatusCode(policy, statusCode))}
                />
                {statusCode}
              </label>
            ))}
          </div>
        </div>

        <div className="space-y-2">
          <div className="text-xs font-medium text-muted-foreground">传输错误</div>
          <div className="flex flex-wrap gap-2">
            {UPSTREAM_RETRY_TRANSPORT_ERRORS.map((kind) => (
              <label
                key={kind}
                className="inline-flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5 text-xs text-secondary-foreground"
              >
                <input
                  type="checkbox"
                  checked={policy.transport_errors.includes(kind)}
                  disabled={disabled}
                  onChange={() =>
                    onChange(toggleRetryTransportError(policy, kind as UpstreamTransportRetryKind))
                  }
                />
                {UPSTREAM_RETRY_TRANSPORT_ERROR_LABELS[kind]}
              </label>
            ))}
          </div>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-3">
        <FormField label="同供应商重试次数">
          <Input
            type="number"
            min={0}
            max={10}
            value={policy.max_retries}
            disabled={disabled}
            onChange={(e) => {
              const next = e.currentTarget.valueAsNumber;
              if (Number.isFinite(next)) onChange({ ...policy, max_retries: next });
            }}
          />
        </FormField>
        <FormField label="重试间隔（毫秒）">
          <Input
            type="number"
            min={0}
            max={60000}
            value={policy.backoff_ms}
            disabled={disabled}
            onChange={(e) => {
              const next = e.currentTarget.valueAsNumber;
              if (Number.isFinite(next)) onChange({ ...policy, backoff_ms: next });
            }}
          />
        </FormField>
        <div className="flex items-center justify-between gap-3 rounded-md border border-border px-3 py-2">
          <div>
            <div className="text-xs font-medium text-foreground">计入熔断</div>
            <div className="text-[11px] text-muted-foreground">关闭时仅最终失败计数。</div>
          </div>
          <Switch
            checked={policy.counts_toward_circuit_breaker}
            disabled={disabled}
            onCheckedChange={(checked) =>
              onChange({ ...policy, counts_toward_circuit_breaker: checked })
            }
          />
        </div>
      </div>
    </div>
  );
}
