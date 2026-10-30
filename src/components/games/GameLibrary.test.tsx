// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Game, OfficialCompatibilityIndex } from "../../types/studio";

vi.mock("../../services/gameArtwork", () => ({
  loadGameIcon: vi.fn(async () => null),
  retainGameIcon: vi.fn(() => vi.fn()),
}));

import { VirtualGameGrid, VirtualHomeGameRow } from "./GameLibrary";

const games: Game[] = Array.from({ length: 1000 }, (_, index) => ({
  id: index + 1,
  path: `/games/${index + 1}`,
  title: `Game ${index + 1}`,
  favorite: false,
  build_id: null,
  play_seconds: 0,
  last_played: null,
  title_id: null,
  game_version: null,
  icon_path: null,
  install_size: null,
  file_status: "Available",
}));

const compatibilityIndex: OfficialCompatibilityIndex = {
  available: true,
  updated_at: null,
  error: null,
  games: [{
    title_id: "PPSA12345",
    title: "Game 1",
    overall: "playable",
    status: "playable",
    preferred_os: "linux",
    os: { linux: "playable" },
    report_counts: { linux: 1 },
    latest_tested: null,
  }],
};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("VirtualHomeGameRow", () => {
  it("updates its virtual window when horizontally scrolled", () => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    render(
      <VirtualHomeGameRow
        games={games.slice(0, 30)}
        selectedId={1}
        onSelect={vi.fn()}
      />,
    );

    const row = document.querySelector<HTMLElement>(".home-game-row")!;
    Object.defineProperty(row, "clientWidth", { configurable: true, value: 500 });
    Object.defineProperty(row, "scrollLeft", { configurable: true, value: 1000 });
    expect(() => fireEvent.scroll(row)).not.toThrow();
    expect(screen.queryByRole("button", { name: "Game 1" })).toBeNull();
  });
});

describe("VirtualGameGrid", () => {
  it("shows official compatibility beside the title in list mode, never on the poster", () => {
    const [matchingGame, ...otherGames] = games;
    render(
      <VirtualGameGrid
        games={[{ ...matchingGame, title_id: "PPSA12345" }, ...otherGames]}
        selectedId={1}
        onSelect={vi.fn()}
        officialCompatibilityIndex={compatibilityIndex}
        mode="list"
      />,
    );

    const badge = screen.getByText("Playable");
    expect(badge.closest(".game-card-title-row")).toBeTruthy();
    expect(document.querySelector(".game-art .compatibility-status-badge")).toBeNull();
    expect(screen.getByRole("button", { name: "Game 1, Linux · Playable compatibility" })).toBeTruthy();
    expect(screen.getByText(/1 report$/)).toBeTruthy();
  });

  it("shows compatibility below the artwork in grid mode", () => {
    render(<VirtualGameGrid games={[{ ...games[0], title_id: "PPSA12345" }]} selectedId={1} onSelect={vi.fn()} officialCompatibilityIndex={compatibilityIndex} mode="grid" />);
    expect(screen.getByText("Playable").closest(".game-card-title-row")).toBeTruthy();
    expect(document.querySelector(".game-art .compatibility-status-badge")).toBeNull();
  });

  it("renders a bounded visible window and advances to the scrolled rows", () => {
    render(
      <VirtualGameGrid
        games={games}
        selectedId={1}
        onSelect={vi.fn()}
        mode="grid"
      />,
    );

    const grid = document.querySelector<HTMLElement>(".game-grid")!;
    expect(grid.querySelectorAll(".game-card").length).toBeLessThan(20);
    expect(screen.getByText("Game 1")).toBeTruthy();
    expect(screen.queryByText("Game 1000")).toBeNull();

    Object.defineProperty(grid, "scrollTop", {
      configurable: true,
      value: 2740,
      writable: true,
    });
    fireEvent.scroll(grid);

    expect(screen.queryByText("Game 1")).toBeNull();
    expect(grid.querySelectorAll(".game-card").length).toBeLessThan(20);
    expect(grid.querySelector(".virtual-spacer")).toBeTruthy();
  });

  it("moves and focuses the selected game through Studio gamepad navigation", () => {
    function Harness() {
      const [selectedId, setSelectedId] = useState(1);
      return (
        <VirtualGameGrid
          games={games}
          selectedId={selectedId}
          onSelect={setSelectedId}
          mode="grid"
        />
      );
    }

    render(<Harness />);
    act(() => {
      window.dispatchEvent(
        new CustomEvent("studio:gamepad-move", {
          detail: { direction: "right", gameId: 1 },
        }),
      );
    });

    const selected = document.querySelector<HTMLButtonElement>(
      '.game-card[data-game-id="2"]',
    );
    expect(selected).toBeTruthy();
    expect(document.activeElement).toBe(selected);
  });
});
