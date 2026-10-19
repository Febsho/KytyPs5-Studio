// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { LauncherSessionModalProps } from "./LauncherSessionModal";
import { LauncherSessionModal } from "./LauncherSessionModal";
import type { Session } from "../../types/studio";

afterEach(cleanup);

const session: Session = {
  id: 8,
  game_id: 3,
  build_id: 2,
  title: "Example Game",
  started_at: 1_700_000_000,
  ended_at: null,
  exit_code: null,
  outcome: null,
  log_path: "/managed/session.log",
  commit_sha: "abc123456789",
  build_type: "Release",
  pid: null,
};

function props(
  overrides: Partial<LauncherSessionModalProps> = {},
): LauncherSessionModalProps {
  return {
    session,
    live: false,
    status: "Exited · code 0",
    runtimeSeconds: 60,
    icon: null,
    log: "last emulator output",
    onDismiss: vi.fn(),
    onStop: vi.fn(),
    onBackToLibrary: vi.fn(),
    onOpenLogs: vi.fn(),
    ...overrides,
  };
}

describe("LauncherSessionModal", () => {
  it("shows the session and does not enable Stop after the process ends", () => {
    const page = props();
    render(<LauncherSessionModal {...page} />);

    expect(screen.getByRole("heading", { name: "Example Game" })).toBeTruthy();
    expect(screen.getByText("last emulator output")).toBeTruthy();
    expect(
      (screen.getByRole("button", { name: /Stop Game/ }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("forwards navigation and log actions", () => {
    const page = props({ live: true });
    render(<LauncherSessionModal {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Back to Library" }));
    expect(page.onBackToLibrary).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: /Open Logs/ }));
    expect(page.onOpenLogs).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Dismiss launcher" }));
    expect(page.onDismiss).toHaveBeenCalledOnce();
  });
});
