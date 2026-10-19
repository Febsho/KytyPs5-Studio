// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsPageProps } from "./SettingsPage";
import { SettingsPage } from "./SettingsPage";
import type { ProfileCapabilities } from "../types/studio";

afterEach(cleanup);

const capabilities: ProfileCapabilities = {
  screen_width: true,
  screen_height: true,
  present_mode: true,
  perf_overlay: true,
  fullscreen: true,
  controller_color: true,
  controller_volume: true,
  controller_vibration: true,
  keyboard_player: true,
  options: ["--screen-width", "--user-name"],
};

function props(overrides: Partial<SettingsPageProps> = {}): SettingsPageProps {
  return {
    globalProfileCapabilities: capabilities,
    globalProfileOptions: { "--screen-width": "1920", "--hdr": "true" },
    setGlobalProfileOptions: vi.fn(),
    globalUnsupported: true,
    gpuDevices: [{ index: 0, name: "Example GPU" }],
    githubTokenAvailable: false,
    githubTokenInput: "secret-value",
    setGithubTokenInput: vi.fn(),
    updateInterval: "24",
    autoUpdateMode: "notify",
    storageReport: {
      categories: [
        {
          name: "Compiler build cache",
          path: "/managed/cache",
          size_bytes: 1024,
        },
      ],
    },
    busy: false,
    activeSessions: false,
    onSaveGlobalDefaults: vi.fn(),
    onSaveGithubToken: vi.fn(),
    onDeleteGithubToken: vi.fn(),
    onUpdateIntervalChange: vi.fn(),
    onAutoUpdateModeChange: vi.fn(),
    onRefreshStorage: vi.fn(),
    onCleanIncompleteBuilds: vi.fn(),
    onClearCompilerCache: vi.fn(),
    onClearCommunityWorktrees: vi.fn(),
    onClearSessionLogs: vi.fn(),
    ...overrides,
  };
}

describe("SettingsPage", () => {
  it("warns about defaults unsupported by the active build and can clear them", () => {
    const page = props();
    render(<SettingsPage {...page} />);

    expect(
      screen.getByText(/Some saved defaults are not supported/),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Clear unsupported defaults" }),
    );
    expect(page.setGlobalProfileOptions).toHaveBeenCalledWith({
      "--screen-width": "1920",
    });
  });

  it("keeps credential storage and cache cleanup actions explicit", () => {
    const page = props();
    render(<SettingsPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Save securely" }));
    expect(page.onSaveGithubToken).toHaveBeenCalledWith("secret-value");
    fireEvent.click(screen.getByRole("button", { name: "Refresh sizes" }));
    expect(page.onRefreshStorage).toHaveBeenCalledOnce();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Clear compiler cache" }),
    );
    expect(confirm).toHaveBeenCalledOnce();
    expect(page.onClearCompilerCache).toHaveBeenCalledOnce();
  });

  it("confirms temporary community checkout cleanup using the measured size", () => {
    const page = props({
      storageReport: {
        categories: [
          {
            name: "Community build worktrees",
            path: "/managed/community-worktrees",
            size_bytes: 2048,
          },
        ],
      },
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<SettingsPage {...page} />);
    fireEvent.click(
      screen.getByRole("button", { name: "Clear temporary community files" }),
    );
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("Remove 2048 B"),
    );
    expect(page.onClearCommunityWorktrees).toHaveBeenCalledOnce();
  });

  it("requires explicit confirmation before deleting session logs", () => {
    const page = props({
      storageReport: {
        categories: [
          { name: "Session logs", path: "/managed/logs", size_bytes: 4096 },
        ],
      },
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<SettingsPage {...page} />);
    fireEvent.click(screen.getByRole("button", { name: "Clear session logs" }));
    expect(confirm).toHaveBeenCalledWith(
      expect.stringContaining("Session history and outcomes will remain"),
    );
    expect(page.onClearSessionLogs).toHaveBeenCalledOnce();
  });

  it("confirms cleanup of measured incomplete installations", () => {
    const page = props({
      storageReport: {
        categories: [
          { name: "Incomplete build installs", path: "/managed/builds", size_bytes: 8192 },
        ],
      },
    });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<SettingsPage {...page} />);
    fireEvent.click(screen.getByRole("button", { name: "Clear incomplete builds" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Remove 8192 B"));
    expect(page.onCleanIncompleteBuilds).toHaveBeenCalledOnce();
  });

  it("disables session-log cleanup while an emulator session is active", () => {
    render(<SettingsPage {...props({ activeSessions: true })} />);
    expect(
      screen
        .getByRole("button", { name: "Clear session logs" })
        .hasAttribute("disabled"),
    ).toBe(true);
  });
});
