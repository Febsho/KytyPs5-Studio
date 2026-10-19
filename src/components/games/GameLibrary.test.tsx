// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Game } from "../../types/studio";

vi.mock("../../services/gameArtwork", () => ({
  loadGameIcon: vi.fn(async () => null),
  retainGameIcon: vi.fn(() => vi.fn()),
}));

import { VirtualGameGrid } from "./GameLibrary";

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

afterEach(cleanup);

describe("VirtualGameGrid", () => {
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
