// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GameLibraryPageProps } from "./GameLibraryPage";
import { GameLibraryPage } from "./GameLibraryPage";
import type { Game } from "../types/studio";

const game: Game = {
  id: 5,
  path: "/games/example",
  title: "Example Game",
  favorite: false,
  build_id: null,
  play_seconds: 300,
  last_played: null,
  title_id: "CUSA00005",
  game_version: "1.00",
  icon_path: null,
  install_size: 1024 ** 3,
  file_status: "Available",
};

afterEach(cleanup);

function props(
  overrides: Partial<GameLibraryPageProps> = {},
): GameLibraryPageProps {
  return {
    games: [game],
    filteredGames: [game],
    currentGame: game,
    gameBackground: null,
    gameIcon: null,
    gameScan: null,
    gameCompatibility: null,
    query: "",
    setQuery: vi.fn(),
    libraryFilter: "all",
    setLibraryFilter: vi.fn(),
    librarySort: "title",
    setLibrarySort: vi.fn(),
    libraryView: "grid",
    setLibraryView: vi.fn(),
    builds: [],
    repos: [],
    profileDirty: false,
    onChoose: vi.fn().mockResolvedValue(undefined),
    onSelectGame: vi.fn(),
    onOpenDetails: vi.fn(),
    onOpenGameFolder: vi.fn(),
    onOpenLatestSessionLog: vi.fn(),
    onLaunch: vi.fn(),
    onToggleFavorite: vi.fn(),
    onSetPreferredBuild: vi.fn(),
    ...overrides,
  };
}

describe("GameLibraryPage", () => {
  it("keeps the selection summary compact and sends details to the dedicated page", () => {
    const page = props();
    render(<GameLibraryPage {...page} />);

    expect(screen.getByRole("heading", { name: "Example Game" })).toBeTruthy();
    expect(screen.queryByText("Profile editor")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Game details" }));
    expect(page.onOpenDetails).toHaveBeenCalledOnce();
  });

  it("forwards search, layout and folder-import controls", () => {
    const page = props({ currentGame: undefined });
    render(<GameLibraryPage {...page} />);

    fireEvent.change(
      screen.getByRole("textbox", { name: "Search your library" }),
      {
        target: { value: "Example" },
      },
    );
    expect(page.setQuery).toHaveBeenCalledWith("Example");
    fireEvent.click(screen.getByRole("button", { name: "List" }));
    expect(page.setLibraryView).toHaveBeenCalledWith("list");
    fireEvent.click(screen.getByRole("button", { name: /Scan folder/ }));
    expect(page.onChoose).toHaveBeenCalledWith("directory");
  });
});
