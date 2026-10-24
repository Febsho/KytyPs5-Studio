// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { open } from "@tauri-apps/plugin-dialog";

const { invokeMock, confirmMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  confirmMock: vi.fn().mockResolvedValue(true),
}));

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
  confirm: confirmMock,
}));

import { App } from "./App";

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
  invokeMock.mockReset();
  confirmMock.mockReset().mockResolvedValue(true);
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

    expect(await screen.findByRole("button", { name: /import game folder/i })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Library" }));
    await waitFor(() => expect(screen.getByText("Your library is empty")).toBeTruthy());
  });

  it("checks a fork only after a manual click and asks before building its update", async () => {
    const repo = {
      id: 9,
      owner: "example",
      name: "kyty-fork",
      url: "https://github.com/example/kyty-fork",
      branch: "main",
      source_path: "/tmp/kyty-fork",
      adapter: "kyty-cmake",
      installed_commit: "abc123",
    };
    const build = {
      id: 12,
      repo_id: repo.id,
      commit_sha: "abc123456789",
      build_type: "Release",
      path: "/managed/builds/12",
      created_at: 1_700_000_000,
      active: true,
      install_size_bytes: 1024,
      validation_status: "verified",
      executable_status: "present",
      configuration: { compiler: "clang", jobs: 4, clean: false, cmake_arguments: [] },
      last_session_at: null,
      last_session_outcome: null,
      last_session_exit_code: null,
    };
    invokeMock.mockImplementation(async (command: string) => {
      if (["list_repos"].includes(command)) return [repo];
      if (["list_builds"].includes(command)) return [build];
      if (["list_games", "recent_sessions", "process_metrics"].includes(command)) return [];
      if (["system_info", "get_setting", "inspect_graphics"].includes(command)) return null;
      if (command === "check_updates") return "Update available: abc123 → def456";
      if (command === "start_update_build") return 77;
      return null;
    });

    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Emulators" }));
    expect(invokeMock).not.toHaveBeenCalledWith("check_updates", expect.anything());
    await user.click(await screen.findByRole("button", { name: "Check updates" }));

    await waitFor(() => expect(confirmMock).toHaveBeenCalledOnce());
    expect(confirmMock.mock.calls[0][0]).toContain("Fetch the upstream commit");
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("start_update_build", {
        id: repo.id,
        activateAfterBuild: false,
        options: { build_type: "Release", jobs: 4, clean: false },
      });
    });
  });

  it("checks for Windows updates at startup without starting the installer", async () => {
    vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0; Win64; x64" });
    invokeMock.mockImplementation(async (command: string) => {
      if (["list_repos", "list_builds", "list_games", "recent_sessions", "process_metrics"].includes(command)) return [];
      if (command === "system_info") return null;
      if (command === "get_setting") return null;
      if (command === "inspect_graphics") return null;
      if (command === "get_global_profile_options") return {};
      if (command === "get_active_profile_capabilities") return {
        screen_width: true,
        screen_height: true,
        present_mode: true,
        perf_overlay: true,
        fullscreen: true,
        controller_color: true,
        controller_volume: true,
        controller_vibration: true,
        keyboard_player: true,
        options: [],
      };
      if (command === "check_app_update") {
        return {
          current_version: "0.3.0",
          latest_version: "0.3.1",
          update_available: true,
          release_url: "https://github.com/Febsho/KytyPs5-Studio/releases/tag/v0.3.1",
          asset_name: "KytyPS5-Studio_0.3.1_x64-setup.exe",
          download_size_bytes: 1024,
          running_appimage: false,
        };
      }
      if (command === "install_app_update") return "Verified NSIS installer started";
      return null;
    });

    render(<App />);

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("check_app_update");
    });
    expect(invokeMock).not.toHaveBeenCalledWith("install_app_update");

  });

  it("passes a Unicode UNC directory from the native picker to the game scanner unchanged", async () => {
    const selectedPath = String.raw`\\server\profile share\ゲーム`;
    vi.mocked(open).mockResolvedValue(selectedPath);
    invokeMock.mockImplementation(async (command: string) => {
      if (["list_repos", "list_builds", "list_games", "recent_sessions", "process_metrics"].includes(command)) return [];
      if (command === "system_info" || command === "get_setting" || command === "inspect_graphics") return null;
      if (command === "scan_games") return { imported: 0, already_registered: 0, rejected: 0 };
      return null;
    });

    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: /import game folder/i }));

    await waitFor(() => {
      expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
      expect(invokeMock).toHaveBeenCalledWith("scan_games", { path: selectedPath });
    });
  });

  it("labels saved GitHub credentials with the Windows credential store", async () => {
    vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0; Win64; x64" });
    invokeMock.mockImplementation(async (command: string) => {
      if (["list_repos", "list_builds", "list_games", "recent_sessions", "process_metrics"].includes(command)) return [];
      if (command === "system_info" || command === "get_setting" || command === "inspect_graphics") return null;
      if (command === "github_token_status") return false;
      if (command === "get_global_profile_options") return {};
      if (command === "get_active_profile_capabilities") return {
        screen_width: true,
        screen_height: true,
        present_mode: true,
        perf_overlay: true,
        fullscreen: true,
        controller_color: true,
        controller_volume: true,
        controller_vibration: true,
        keyboard_player: true,
        options: [],
      };
      return null;
    });

    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "Settings" }));
    await user.type(screen.getByPlaceholderText("Paste token (optional)"), "test-token");
    await user.click(screen.getByRole("button", { name: "Save securely" }));

    expect(await screen.findByText("GitHub token stored in Windows Credential Manager")).toBeTruthy();
    expect(invokeMock).toHaveBeenCalledWith("save_github_token", { token: "test-token" });
  });
});
