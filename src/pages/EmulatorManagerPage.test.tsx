// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EmulatorManagerPageProps } from "./EmulatorManagerPage";
import { EmulatorManagerPage } from "./EmulatorManagerPage";
import type { Build, Game, Repo } from "../types/studio";

const repo: Repo = {
  id: 1,
  owner: "KytyPS5",
  name: "KytyPS5",
  url: "https://github.com/KytyPS5/KytyPS5",
  branch: "main",
  source_path: "/tmp/kyty-source",
  adapter: "official",
  installed_commit: "abc123",
};
const build: Build = {
  id: 2,
  repo_id: repo.id,
  commit_sha: "abc123456789",
  build_type: "Release",
  path: "/managed/builds/2",
  created_at: 1_700_000_000,
  active: false,
  install_size_bytes: 1024,
  validation_status: "verified",
  executable_status: "present",
  configuration: { compiler: "clang", jobs: 8, clean: false, cmake_arguments: ["-DCMAKE_BUILD_TYPE=Release"] },
  last_session_at: null,
  last_session_outcome: null,
  last_session_exit_code: null,
};
const game: Game = {
  id: 3,
  path: "/games/example",
  title: "Example Game",
  favorite: false,
  build_id: build.id,
  play_seconds: 0,
  last_played: null,
  title_id: null,
  game_version: null,
  icon_path: null,
  install_size: null,
  file_status: "Available",
};

function props(
  overrides: Partial<EmulatorManagerPageProps> = {},
): EmulatorManagerPageProps {
  return {
    builds: [build],
    repos: [repo],
    games: [],
    sessions: [],
    selectedLog: "",
    busy: false,
    onNewBuild: vi.fn(),
    onOpenForkManager: vi.fn(),
    onOpenBuildFolder: vi.fn(),
    onVerifyBuild: vi.fn(),
    onCheckUpdates: vi.fn(),
    onRebuild: vi.fn(),
    onActivateBuild: vi.fn(),
    onOpenBuildLog: vi.fn(),
    onRemoveBuild: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("EmulatorManagerPage", () => {
  it("shows build details and forwards build actions", () => {
    const page = props();
    render(<EmulatorManagerPage {...page} />);

    expect(screen.getByText(/abc1234/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    expect(page.onOpenBuildFolder).toHaveBeenCalledWith(build.id);
    fireEvent.click(screen.getByRole("button", { name: "Verify CLI" }));
    expect(page.onVerifyBuild).toHaveBeenCalledWith(build.id);
  });

  it("protects builds referenced by registered games", () => {
    render(<EmulatorManagerPage {...props({ games: [game] })} />);

    const remove = screen.getByRole("button", { name: "Remove" });
    expect((remove as HTMLButtonElement).disabled).toBe(true);
    expect(remove.getAttribute("title")).toContain(
      "game still prefers this build",
    );
  });

  it("compares recorded configuration and validation for two builds", () => {
    const second = {
      ...build,
      id: 4,
      commit_sha: "fedcba987654",
      configuration: { compiler: "gcc", jobs: 4, clean: true, cmake_arguments: ["-DCMAKE_BUILD_TYPE=Debug"] },
    };
    render(<EmulatorManagerPage {...props({ builds: [build, second] })} />);

    const compareButtons = screen.getAllByRole("button", { name: "Compare" });
    fireEvent.click(compareButtons[0]);
    fireEvent.click(compareButtons[1]);

    expect(screen.getByRole("heading", { name: "Build comparison" })).toBeTruthy();
    expect(screen.getByText("clang")).toBeTruthy();
    expect(screen.getByText("gcc")).toBeTruthy();
    expect(screen.getAllByText("-DCMAKE_BUILD_TYPE=Debug")).toHaveLength(2);
  });

  it("offers navigation when no verified builds exist", () => {
    const page = props({ builds: [] });
    render(<EmulatorManagerPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Open Fork Manager" }));
    expect(page.onOpenForkManager).toHaveBeenCalledOnce();
  });
});
