import { describe, expect, it } from "vitest";
import { USAGE_TABLE_TAB_ITEMS } from "../constants";

describe("usage/constants", () => {
  it("registers remote usage tab", () => {
    expect(USAGE_TABLE_TAB_ITEMS).toContainEqual({
      key: "remoteUsage",
      label: "远端用量",
    });
  });
});
