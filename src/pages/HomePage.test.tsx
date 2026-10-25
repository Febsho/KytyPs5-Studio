// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Game, OfficialCompatibilityIndex } from "../types/studio";
import type { HomePageProps } from "./HomePage";
import { HomePage } from "./HomePage";

vi.mock("../services/gameArtwork", () => ({
  loadGameIcon: vi.fn(async () => null),
  retainGameIcon: vi.fn(() => vi.fn()),
}));

function props(overrides: Partial<HomePageProps> = {}): HomePageProps {
  return {
    games: [],
    featuredGame: null,
    featuredBackground: null,
    builds: [],
    repos: [],
    message: "",
    setPage: vi.fn(),
    choose: vi.fn().mockResolvedValue(undefined),
    launchGame: vi.fn().mockResolvedValue(undefined),
    selectGameWithGuard: vi.fn(),
    setGameDetailsTab: vi.fn(),
    setHomeSelection: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("HomePage", () => {
  it("offers game import and library navigation when the library is empty", () => {
    const page = props();
    render(<HomePage {...page} />);

    expect(screen.getByRole("button", { name: /import game folder/i })).toBeTruthy();
    fireEvent.click(
      screen.getAllByRole("button", { name: /import game folder/i })[0],
    );
    expect(page.choose).toHaveBeenCalledWith("directory");
    fireEvent.click(screen.getByRole("button", { name: "Library" }));
    expect(page.setPage).toHaveBeenCalledWith("library");
  });

  it("shows the compatibility status in the bottom game details, not on its cover", () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    const game: Game = {
      id: 3, path: "/games/example", title: "Example", favorite: false,
      build_id: null, play_seconds: 0, last_played: null,
      title_id: "PPSA12345", game_version: "01.001.000", icon_path: null,
      install_size: null, file_status: "Available",
    };
    const officialCompatibilityIndex: OfficialCompatibilityIndex = {
      available: true, updated_at: null, error: null,
      games: [{
        title_id: "PPSA12345", title: "Example", overall: "in-game", status: "in-game",
        preferred_os: "linux", os: { linux: "in-game" }, report_counts: {}, latest_tested: null,
      }],
    };
    render(<HomePage {...props({ games: [game], featuredGame: game, officialCompatibilityIndex })} />);

    const badge = screen.getByText("In-Game");
    expect(badge.classList.contains("tone-in-game")).toBe(true);
    expect(badge.closest(".home-feature")).toBeTruthy();
    expect(document.querySelector(".home-game-art .compatibility-status-badge")).toBeNull();
    expect(screen.queryByRole("button", { name: /technical specs/i })).toBeNull();
  });

  it("shows the featured game's preferred emulator instead of the globally active build", () => {
    const game: Game = {
      id: 7, path: "/games/preferred", title: "Preferred", favorite: false,
      build_id: 2, play_seconds: 0, last_played: null, title_id: null,
      game_version: null, icon_path: null, install_size: null, file_status: "Available",
    };
    const activeBuild = { id: 1, repo_id: 3, commit_sha: "active123456", active: true } as HomePageProps["active"];
    const preferredBuild = { id: 2, repo_id: 4, commit_sha: "preferred123", active: false } as HomePageProps["builds"][number];
    const repos = [
      { id: 3, owner: "KytyPS5", name: "KytyPS5", url: "", branch: "main", source_path: null, adapter: "", installed_commit: null },
      { id: 4, owner: "ExampleFork", name: "KytyPS5", url: "", branch: "main", source_path: null, adapter: "", installed_commit: null },
    ];
    render(<HomePage {...props({ games: [game], featuredGame: game, active: activeBuild, builds: [activeBuild!, preferredBuild], repos })} />);
    expect(screen.getByText(/ExampleFork.*#preferre/i)).toBeTruthy();
    expect(screen.queryByText(/KytyPS5.*#active123/i)).toBeNull();
  });
});
