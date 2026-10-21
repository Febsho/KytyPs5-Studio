// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiagnosticsPageProps } from "./DiagnosticsPage";
import { DiagnosticsPage } from "./DiagnosticsPage";

function props(
  overrides: Partial<DiagnosticsPageProps> = {},
): DiagnosticsPageProps {
  return {
    system: null,
    graphics: null,
    processMetrics: [],
    sessions: [],
    selectedLog: "",
    selectedLogPath: null,
    logFilter: "All",
    setLogFilter: vi.fn(),
    onSelectSessionLog: vi.fn(),
    onExportLog: vi.fn(),
    onExportCrashReport: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("DiagnosticsPage", () => {
  it("reports empty sessions and forwards session log selection", () => {
    const session = {
      id: 4,
      game_id: 2,
      build_id: 3,
      title: "Example Game",
      started_at: 1_700_000_000,
      ended_at: 1_700_000_060,
      exit_code: 0,
      outcome: "exited",
      log_path: "/managed/session.log",
      commit_sha: "abc123",
      build_type: "Release",
      pid: 99,
    };
    const page = props({ sessions: [session] });
    const { rerender } = render(<DiagnosticsPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: /Example Game/ }));
    expect(page.onSelectSessionLog).toHaveBeenCalledWith(session);

    rerender(<DiagnosticsPage {...page} sessions={[]} />);
    expect(screen.getByText("No sessions recorded")).toBeTruthy();
  });

  it("filters logs and offers export only when a log path is selected", () => {
    const session = {
      id: 4,
      game_id: 2,
      build_id: 3,
      title: "Example Game",
      started_at: 1_700_000_000,
      ended_at: 1_700_000_060,
      exit_code: 0,
      outcome: "exited",
      log_path: "/managed/session.log",
      commit_sha: "abc123",
      build_type: "Release",
      pid: 99,
    };
    const page = props({
      sessions: [session],
      selectedLog: "info: started\nerror: device lost",
      selectedLogPath: "/managed/session.log",
      logFilter: "Error",
    });
    render(<DiagnosticsPage {...page} />);

    expect(screen.getByText("error: device lost")).toBeTruthy();
    expect(screen.queryByText("info: started")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Export log" }));
    expect(page.onExportLog).toHaveBeenCalledOnce();
  });
});
