import { describe, expect, it } from "vitest";
import {
  normalizeRepoPath,
  repoKey,
  repositoryWebUrl,
  sourceHint,
  sourceKey,
} from "../skillSources";

describe("utils/skillSources extra coverage", () => {
  it("builds source and repo keys", () => {
    expect(
      sourceKey({
        source_git_url: "https://github.com/acme/repo.git",
        source_branch: "main",
        source_subdir: "skills/alpha",
      })
    ).toBe("https://github.com/acme/repo.git#main:skills/alpha");
    expect(
      repoKey({
        source_git_url: "https://github.com/acme/repo.git",
        source_branch: "main",
      })
    ).toBe("https://github.com/acme/repo.git#main");
  });

  it("normalizes ssh, https, local, and fallback repo paths", () => {
    expect(normalizeRepoPath("git@github.com:acme/repo.git")).toBe("acme/repo");
    expect(normalizeRepoPath("https://github.com/acme/repo.git")).toBe("acme/repo");
    expect(normalizeRepoPath(" local://alpha ")).toBe("");
    expect(normalizeRepoPath("github.com/acme/repo.git")).toBe("acme/repo");
  });

  it("builds repository web urls and source hints", () => {
    expect(repositoryWebUrl("git@github.com:acme/repo.git")).toBe("https://github.com/acme/repo");
    expect(repositoryWebUrl("https://github.com/acme/repo.git?x=1#frag")).toBe(
      "https://github.com/acme/repo"
    );
    expect(repositoryWebUrl("not-a-url")).toBeNull();
    expect(
      sourceHint({
        source_git_url: " https://github.com/acme/repo.git ",
        source_branch: " main ",
        source_subdir: " skills/alpha ",
      })
    ).toBe("https://github.com/acme/repo.git#main:skills/alpha");
    expect(sourceHint({ source_git_url: "", source_branch: "main", source_subdir: "skills" })).toBe(
      ""
    );
  });
});
