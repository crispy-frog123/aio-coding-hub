// Usage: Frontend wrapper for status.input.im model service status.

import {
  commands,
  type ServiceStatusCellKind,
  type ServiceStatusResult as GeneratedServiceStatusResult,
} from "../../generated/bindings";
import { invokeGeneratedIpc, mapGeneratedCommandResponse } from "../generatedIpc";
import { narrowGeneratedStringUnion } from "../generatedTypeUtils";

const CELL_KIND_VALUES = [
  "green",
  "yellow",
  "red",
  "gray",
] as const satisfies readonly ServiceStatusCellKind[];

export type { ServiceStatusCellKind };
export type ServiceStatusResult = GeneratedServiceStatusResult;

function normalizeCellKind(value: string): ServiceStatusCellKind {
  return narrowGeneratedStringUnion(value, CELL_KIND_VALUES, "service_status.kind");
}

function normalizeResult(value: GeneratedServiceStatusResult): ServiceStatusResult {
  if (value.snapshot == null) return value;
  return {
    ...value,
    snapshot: {
      ...value.snapshot,
      response: {
        ...value.snapshot.response,
        services: value.snapshot.response.services.map((service) => ({
          ...service,
          latest_kind: normalizeCellKind(service.latest_kind),
          last: service.last
            ? { ...service.last, kind: normalizeCellKind(service.last.kind) }
            : null,
          history: service.history.map((probe) => ({
            ...probe,
            kind: normalizeCellKind(probe.kind),
          })),
        })),
      },
    },
  };
}

export async function serviceStatusFetch() {
  return invokeGeneratedIpc<ServiceStatusResult>({
    title: "读取模型服务状态失败",
    cmd: "service_status_fetch",
    args: {},
    invoke: async () =>
      mapGeneratedCommandResponse(await commands.serviceStatusFetch(), normalizeResult),
  });
}
