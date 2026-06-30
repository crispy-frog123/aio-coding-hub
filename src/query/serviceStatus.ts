// Usage: React Query hook for status.input.im model service status.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { serviceStatusFetch } from "../services/usage/serviceStatus";
import { serviceStatusKeys } from "./keys";

export function useServiceStatusQuery(options?: {
  enabled?: boolean;
  refetchIntervalMs?: number | false;
}) {
  return useQuery({
    queryKey: serviceStatusKeys.current(),
    queryFn: serviceStatusFetch,
    enabled: options?.enabled ?? true,
    placeholderData: keepPreviousData,
    refetchInterval: options?.refetchIntervalMs ?? false,
  });
}
