// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomePageProps } from "./HomePage";
import { HomePage } from "./HomePage";

function props(overrides: Partial<HomePageProps> = {}): HomePageProps {
  return {
    games: [],
    featuredGame: null,
    featuredBackground: null,
    repos: [],
    system: null,
    graphics: null,
    specsOpen: false,
    setSpecsOpen: vi.fn(),
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

  it("opens diagnostics from the technical specifications drawer", () => {
    const page = props({ specsOpen: true });
    render(<HomePage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Open Diagnostics" }));
    expect(page.setSpecsOpen).toHaveBeenCalledWith(false);
    expect(page.setPage).toHaveBeenCalledWith("diagnostics");
  });
});
