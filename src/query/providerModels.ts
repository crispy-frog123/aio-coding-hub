import { useMutation } from "@tanstack/react-query";
import {
  providerModelProbe,
  providerModelsList,
  type ProviderModelProbeResult,
  type ProviderModelsResult,
} from "../services/providers/providerModels";

export function useProviderModelsListMutation() {
  return useMutation<ProviderModelsResult, Error, { providerId: number; baseUrl?: string | null }>({
    mutationFn: (input) => providerModelsList(input.providerId, input.baseUrl),
  });
}

export function useProviderModelProbeMutation() {
  return useMutation<
    ProviderModelProbeResult,
    Error,
    { providerId: number; model: string; baseUrl?: string | null }
  >({
    mutationFn: providerModelProbe,
  });
}
