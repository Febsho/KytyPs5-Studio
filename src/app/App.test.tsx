// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: vi.fn().mockResolvedValue(() => {}) }),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));

import { App } from "./App";

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
});

describe("App startup", () => {
  it("renders and navigates with an empty game library", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (["list_repos", "list_builds", "list_games", "recent_sessions", "process_metrics"].includes(command)) return [];
      if (command === "system_info") return null;
      if (command === "get_setting") return null;
      if (command === "inspect_graphics") return null;
      return null;
    });

    const user = userEvent.setup();
    render(<App />);

    expect(await screen.findByText("No imported games")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Library" }));
    await waitFor(() => expect(screen.getByText("Your library is empty")).toBeTruthy());
  });
});
