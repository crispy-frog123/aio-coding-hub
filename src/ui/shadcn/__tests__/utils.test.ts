import { describe, expect, it } from "vitest";
import { cn } from "../utils";

describe("ui/shadcn/utils", () => {
  it("re-exports cn", () => {
    expect(cn("a", false && "b", "c")).toBe("a c");
  });
});
