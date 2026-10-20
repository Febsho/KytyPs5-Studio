import { afterEach, describe, expect, it, vi } from "vitest";
import type { Build, Game, OfficialCompatibilityDetails, Repo } from "../types/studio";
import { currentCompatibilityOS, recommendVerifiedBuild } from "./compatibility";

const game: Game = {
  id: 1, path: "/games/test", title: "Test", favorite: false, build_id: null,
  play_seconds: 0, last_played: null, title_id: "PPSA12345", game_version: null,
  icon_path: null, install_size: null, file_status: "Available",
};
const repo: Repo = {
  id: 2, owner: "KytyPS5", name: "KytyPS5", url: "https://github.com/KytyPS5/KytyPS5",
  branch: "main", source_path: "/sources/2", adapter: "verified", installed_commit: "abcdef0123456789", removed: false,
};
const build: Build = {
  id: 3, repo_id: 2, commit_sha: "abcdef0123456789", build_type: "Release", path: "/builds/3",
  created_at: 1, active: false, install_size_bytes: null, validation_status: "verified",
  executable_status: "Executable present",
  configuration: { compiler: null, jobs: null, clean: null, cmake_arguments: [] },
  last_session_at: null, last_session_outcome: "error", last_session_exit_code: 65,
};
const report = (overrides: Partial<OfficialCompatibilityDetails["reports"][number]> = {}) => ({
  slug: "test-linux", title: "Test", titleId: "PPSA12345", status: "in-game",
  testedVersion: "KytyPS5 Linux build abcdef0", testedDate: "2026-10-19", os: "linux", notes: "Reached gameplay",
  ...overrides,
});
const details = (reports: OfficialCompatibilityDetails["reports"]): OfficialCompatibilityDetails => ({
  version: 2, key: "PPSA12345", title: "Test", titleId: "PPSA12345", reports,
});

describe("host operating system detection", () => {
  afterEach(() => vi.unstubAllGlobals());

  it.each([
    ["Windows NT 10.0; Win64; x64", "windows"],
    ["Macintosh; Intel Mac OS X 14_0", "macos"],
    ["X11; Linux x86_64", "linux"],
  ] as const)("maps %s to %s", (userAgent, expected) => {
    vi.stubGlobal("navigator", { userAgent });
    expect(currentCompatibilityOS()).toBe(expected);
  });
});

describe("Smart Fork & Build Advisor", () => {
  it("recommends a verified installed build only when an exact Linux report names its commit", () => {
    const recommendation = recommendVerifiedBuild(game, details([report()]), [build], [repo]);
    expect(recommendation?.build?.id).toBe(3);
    expect(recommendation?.status).toBe("in-game");
  });

  it("does not infer a winner from an ambiguous comparison or a different title ID", () => {
    const ambiguous = report({ testedVersion: "Official abcdef0 compared with fork 1234567" });
    const wrongTitle = report({ titleId: "PPSA99999" });
    expect(recommendVerifiedBuild(game, details([ambiguous, wrongTitle]), [build], [repo])).toBeNull();
  });

  it("does not treat unverified or missing executables as recommendations", () => {
    const unavailableSource = { ...repo, source_path: null, installed_commit: null };
    expect(recommendVerifiedBuild(game, details([report()]), [{ ...build, validation_status: "not-checked" }], [unavailableSource])).toBeNull();
    expect(recommendVerifiedBuild(game, details([report()]), [{ ...build, executable_status: "Missing" }], [unavailableSource])).toBeNull();
  });

  it("never recommends a build whose exact report says it does not boot", () => {
    expect(recommendVerifiedBuild(game, details([report({ status: "doesnt-boot" })]), [build], [repo])).toBeNull();
  });

  it("can direct the user to build an exact matching checked-out source commit", () => {
    const recommendation = recommendVerifiedBuild(game, details([report()]), [], [repo]);
    expect(recommendation?.kind).toBe("build");
    expect(recommendation?.repo.id).toBe(2);
  });

  it("requires Linux-specific evidence even when another OS has a higher status", () => {
    const recommendation = recommendVerifiedBuild(game, details([
      report({ os: "windows", status: "in-game" }),
      report({ os: "linux", status: "logo" }),
    ]), [build], [repo]);
    expect(recommendation?.report.os).toBe("linux");
    expect(recommendation?.status).toBe("logo");
  });

  it("uses Windows-specific evidence for Windows recommendations", () => {
    const recommendation = recommendVerifiedBuild(game, details([
      report({ os: "linux", status: "playable" }),
      report({ os: "Windows", status: "in-game" }),
    ]), [build], [repo], "windows");
    expect(recommendation?.report.os).toBe("Windows");
    expect(recommendation?.status).toBe("in-game");
  });
});
