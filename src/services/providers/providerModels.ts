import {
  commands,
  type ProviderModelInfo,
  type ProviderModelProbeResult,
  type ProviderModelsResult,
} from "../../generated/bindings";
import { invokeGeneratedIpc, type GeneratedCommandResult } from "../generatedIpc";
import { FeValidationError } from "../../utils/errors";
import { validateProviderId } from "./providers";

export type { ProviderModelInfo, ProviderModelProbeResult, ProviderModelsResult };

export const MAX_PROVIDER_MODEL_ID_CHARS = 200;

export function validateProviderModelId(model: string): string {
  const normalized = model.trim();
  if (!normalized) {
    throw new FeValidationError("SEC_INVALID_INPUT: model is required");
  }
  if (Array.from(normalized).length > MAX_PROVIDER_MODEL_ID_CHARS) {
    throw new FeValidationError(
      `SEC_INVALID_INPUT: model must contain at most ${MAX_PROVIDER_MODEL_ID_CHARS} characters`
    );
  }
  if (/\p{Cc}/u.test(normalized)) {
    throw new FeValidationError("SEC_INVALID_INPUT: model contains control characters");
  }
  return normalized;
}

function normalizeBaseUrl(baseUrl: string | null | undefined): string | null {
  const normalized = baseUrl?.trim() ?? "";
  return normalized || null;
}

export async function providerModelsList(
  providerId: number,
  baseUrl?: string | null
): Promise<ProviderModelsResult> {
  const normalizedProviderId = validateProviderId(providerId);
  const normalizedBaseUrl = normalizeBaseUrl(baseUrl);

  return invokeGeneratedIpc<ProviderModelsResult>({
    title: "获取 Provider 模型列表失败",
    cmd: "provider_models_list",
    args: { providerId: normalizedProviderId, baseUrl: normalizedBaseUrl },
    invoke: () =>
      commands.providerModelsList(normalizedProviderId, normalizedBaseUrl) as Promise<
        GeneratedCommandResult<ProviderModelsResult>
      >,
  });
}

export async function providerModelProbe(input: {
  providerId: number;
  model: string;
  baseUrl?: string | null;
}): Promise<ProviderModelProbeResult> {
  const providerId = validateProviderId(input.providerId);
  const model = validateProviderModelId(input.model);
  const baseUrl = normalizeBaseUrl(input.baseUrl);

  return invokeGeneratedIpc<ProviderModelProbeResult>({
    title: "检测 Provider 模型失败",
    cmd: "provider_model_probe",
    args: { providerId, model, baseUrl },
    invoke: () =>
      commands.providerModelProbe(providerId, model, baseUrl) as Promise<
        GeneratedCommandResult<ProviderModelProbeResult>
      >,
  });
}
