// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GameDetailsPageProps } from "./GameDetailsPage";
import { GameDetailsPage } from "./GameDetailsPage";
import type { Build, Game, GameContent, Repo } from "../types/studio";

const game: Game = {
  id: 42,
  path: "/games/example",
  title: "Example Game",
  favorite: false,
  build_id: null,
  play_seconds: 120,
  last_played: null,
  title_id: "CUSA00001",
  game_version: "1.00",
  icon_path: null,
  install_size: 1024,
  file_status: "Available",
};

function props(
  overrides: Partial<GameDetailsPageProps> = {},
): GameDetailsPageProps {
  return {
    game,
    tab: "Overview",
    setTab: vi.fn(),
    profileDirty: false,
    builds: [],
    repos: [],
    gameProfileEditor: <div>Profile editor</div>,
    gameCompatibility: null,
    manualOutcome: "reached_menu",
    setManualOutcome: vi.fn(),
    manualNotes: "",
    setManualNotes: vi.fn(),
    compatibilityObservations: [],
    sessions: [],
    onBack: vi.fn(),
    onLaunch: vi.fn(),
    onOpenGameFolder: vi.fn(),
    onRefreshMetadata: vi.fn(),
    onRemoveEntry: vi.fn(),
    onSetPreferredBuild: vi.fn(),
    onRecordObservation: vi.fn(),
    onOpenSessionLog: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("GameDetailsPage", () => {
  it("keeps overview actions tied to the selected game", () => {
    const page = props();
    render(<GameDetailsPage {...page} />);

    expect(screen.getByRole("heading", { name: "Example Game" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Play game/i }));
    expect(page.onLaunch).toHaveBeenCalledWith(game.id);
    fireEvent.click(screen.getByRole("button", { name: "Open game folder" }));
    expect(page.onOpenGameFolder).toHaveBeenCalledOnce();
  });

  it("uses the standard Kyty PS5 maintainer and commit name in the overview", () => {
    const repo: Repo = {
      id: 1, owner: "Jetsku", name: "KytyPS5", url: "https://github.com/Jetsku/KytyPS5",
      branch: "main", source_path: null, adapter: "kytyps5", installed_commit: null,
    };
    const build: Build = {
      id: 7, repo_id: repo.id, commit_sha: "12345678abcdef00", build_type: "Release",
      path: "/builds/7", created_at: 0, active: true, install_size_bytes: null,
      validation_status: "verified", executable_status: "Executable present",
      configuration: { compiler: null, jobs: null, clean: null, cmake_arguments: [] },
      last_session_at: null, last_session_outcome: null, last_session_exit_code: null,
    };
    render(<GameDetailsPage {...props({ builds: [build], repos: [repo] })} />);

    expect(screen.getAllByText("Kyty PS5 · Jetsku · #12345678")).toHaveLength(2);
    expect(document.querySelector(".game-overview-panel .game-overview-heading")).toBeTruthy();
  });

  it("shows shader pipeline progress and can stop a precompile", () => {
    const onStopPrecompileShaders = vi.fn(async () => {});
    render(<GameDetailsPage {...props({
      tab: "Settings",
      shaderPrecompileSupport: { full_precompile: true, background_precompile: false, background_enabled: false, cache_directory: "/cache" },
      shaderPrecompileBusy: true,
      shaderPrecompileProgress: { shaders_total: 28_806, pipelines_compiled: 8_400, pipelines_total: 95_327, live_workers: 12 },
      onPrecompileShaders: vi.fn(async () => {}),
      onStopPrecompileShaders,
    })} />);

    const stats = document.querySelectorAll(".shader-precompile-progress-stats span");
    expect(stats[0]?.textContent).toContain((28_806).toLocaleString());
    expect(stats[1]?.textContent).toContain(`${(8_400).toLocaleString()} / ${(95_327).toLocaleString()}`);
    expect(stats[2]?.textContent).toContain("12 live compiler workers");
    fireEvent.click(screen.getByRole("button", { name: "Stop precompile" }));
    expect(onStopPrecompileShaders).toHaveBeenCalledOnce();
  });

  it("exposes explicit compatibility outcomes and records the user's observation", () => {
    const page = props({ tab: "Compatibility" });
    render(<GameDetailsPage {...page} />);

    fireEvent.change(screen.getByLabelText("Outcome observed"), {
      target: { value: "playable_with_issues" },
    });
    expect(page.setManualOutcome).toHaveBeenCalledWith("playable_with_issues");
    fireEvent.click(
      screen.getByRole("button", { name: "Record test observation" }),
    );
    expect(page.onRecordObservation).toHaveBeenCalledOnce();
  });

  it("shows patch switches and persists changes through the patch manager", async () => {
    const content: GameContent = {
      trophies: [],
      trophy_unlock_state: "unavailable",
      patches: [{ name: "Disable GI", enabled: false, title_id: "CUSA00001", version: "1.00" }],
      patch_source: "/emulator/_Patches/CUSA00001.json",
      patch_error: null,
    };
    const onSetGamePatchModEnabled = vi.fn(async () => ({
      content: { ...content, patches: [{ ...content.patches[0], enabled: true }] },
      patchJson: '{"mods":[{"name":"Disable GI","enabled":true}]}',
    }));
    render(<GameDetailsPage {...props({
      tab: "Patches",
      onInspectContent: vi.fn(async () => content),
      onSetGamePatchModEnabled,
    })} />);

    const toggle = await screen.findByRole("checkbox");
    expect((toggle as HTMLInputElement).checked).toBe(false);
    fireEvent.click(toggle);
    expect(onSetGamePatchModEnabled).toHaveBeenCalledWith(game.id, 0, true);
    expect(await screen.findByRole("checkbox")).toHaveProperty("checked", true);
  });
});
