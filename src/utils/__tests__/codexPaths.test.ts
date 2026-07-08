import { describe, expect, it } from "vitest";
import { buildConfigTomlPath, normalizeCustomCodexHome } from "../codexPaths";

describe("utils/codexPaths", () => {
  it("normalizes blank values and strips trailing config.toml", () => {
    expect(normalizeCustomCodexHome("   ")).toBe("");
    expect(normalizeCustomCodexHome(" C:\\Users\\me\\.codex\\config.toml ")).toBe(
      "C:\\Users\\me\\.codex"
    );
    expect(normalizeCustomCodexHome("/home/me/.codex/config.toml")).toBe("/home/me/.codex");
  });

  it("builds config.toml paths for windows and posix separators", () => {
    expect(buildConfigTomlPath("C:\\Users\\me\\.codex")).toBe("C:\\Users\\me\\.codex\\config.toml");
    expect(buildConfigTomlPath("/home/me/.codex")).toBe("/home/me/.codex/config.toml");
    expect(buildConfigTomlPath("/home/me/.codex/")).toBe("/home/me/.codex/config.toml");
    expect(buildConfigTomlPath("")).toBe("");
  });
});
