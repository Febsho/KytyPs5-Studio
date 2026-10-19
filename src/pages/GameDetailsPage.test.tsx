// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GameDetailsPageProps } from "./GameDetailsPage";
import { GameDetailsPage } from "./GameDetailsPage";
import type { Game } from "../types/studio";

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
});
