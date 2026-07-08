import { describe, expect, it } from "vitest";
import {
  parseYyyyMmDd,
  unixSecondsAtLocalStartOfDay,
  unixSecondsAtLocalStartOfNextDay,
} from "../localDate";

describe("utils/localDate extra coverage", () => {
  it("rejects malformed inputs", () => {
    expect(parseYyyyMmDd("2020-1-01")).toBeNull();
    expect(parseYyyyMmDd("2020-13-01")).toBeNull();
    expect(parseYyyyMmDd("2020-01-32")).toBeNull();
    expect(unixSecondsAtLocalStartOfDay("bad")).toBeNull();
    expect(unixSecondsAtLocalStartOfNextDay("bad")).toBeNull();
  });

  it("next-day helper advances by at least one local day", () => {
    const start = unixSecondsAtLocalStartOfDay("2024-02-29");
    const next = unixSecondsAtLocalStartOfNextDay("2024-02-29");
    expect(start).not.toBeNull();
    expect(next).not.toBeNull();
    expect((next as number) - (start as number)).toBeGreaterThanOrEqual(23 * 60 * 60);
  });
});
