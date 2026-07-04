import { describe, expect, it } from "vitest";
import { parseAttemptsJson } from "../attemptsJson";

describe("services/gateway/attemptsJson", () => {
  it("parses a valid attempts array", () => {
    const attempts = parseAttemptsJson(
      JSON.stringify([
        {
          provider_id: 1,
          provider_name: "Provider A",
          base_url: "https://example.com",
          outcome: "success",
          status: 200,
          timeout_secs: 30,
        },
      ])
    );

    expect(attempts).toHaveLength(1);
    expect(attempts?.[0]).toMatchObject({
      provider_id: 1,
      provider_name: "Provider A",
      outcome: "success",
      timeout_secs: 30,
    });
  });

  it("returns null for invalid JSON", () => {
    expect(parseAttemptsJson("not json")).toBeNull();
  });

  it("returns null for non-array JSON", () => {
    expect(parseAttemptsJson('{"provider_id":1}')).toBeNull();
    expect(parseAttemptsJson('"plain"')).toBeNull();
  });

  it("returns null for null, undefined, and empty input", () => {
    expect(parseAttemptsJson(null)).toBeNull();
    expect(parseAttemptsJson(undefined)).toBeNull();
    expect(parseAttemptsJson("")).toBeNull();
  });
});
