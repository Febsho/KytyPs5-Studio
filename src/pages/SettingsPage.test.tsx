// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SettingsPageProps } from "./SettingsPage";
import { SettingsPage } from "./SettingsPage";
import type { ProfileCapabilities } from "../types/studio";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

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
  it("describes Windows storage and credential services on Windows", () => {
    vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0; Win64; x64" });
    render(<SettingsPage {...props()} />);

    expect(screen.getByText(/LocalAppData.*Roaming AppData/)).toBeTruthy();
    expect(screen.getByText(/Windows Credential Manager/)).toBeTruthy();
  });

  it("requires the user to install a Windows update and blocks it during a game session", () => {
    vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0; Win64; x64" });
    render(<SettingsPage {...props({
      activeSessions: true,
      appUpdate: {
        current_version: "0.3.0",
        latest_version: "0.3.1",
        update_available: true,
        release_url: "https://github.com/Febsho/KytyPs5-Studio/releases/tag/v0.3.1",
        asset_name: "KytyPS5-Studio_0.3.1_x64-setup.exe",
        download_size_bytes: 1024,
        running_appimage: false,
      },
      onInstallAppUpdate: vi.fn(),
    })} />);

    expect(screen.getByText(/opens only after you choose to install it/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Install Verified Update" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByText(/Finish active builds and close emulator sessions/)).toBeTruthy();
  });

  it("starts a verified Windows update only when the user selects install", () => {
    vi.stubGlobal("navigator", { userAgent: "Windows NT 10.0; Win64; x64" });
    const onInstallAppUpdate = vi.fn();
    render(<SettingsPage {...props({
      appUpdate: {
        current_version: "0.3.0",
        latest_version: "0.3.1",
        update_available: true,
        release_url: "https://github.com/Febsho/KytyPs5-Studio/releases/tag/v0.3.1",
        asset_name: "KytyPS5-Studio_0.3.1_x64-setup.exe",
        download_size_bytes: 1024,
        running_appimage: false,
      },
      onInstallAppUpdate,
    })} />);

    fireEvent.click(screen.getByRole("button", { name: "Install Verified Update" }));

    expect(onInstallAppUpdate).toHaveBeenCalledOnce();
  });

  it("surfaces credential-store errors instead of reporting anonymous access", () => {
    render(
      <SettingsPage
        {...props({ githubTokenError: "Credential Manager access was denied" })}
      />,
    );

    expect(screen.getByRole("alert").textContent).toBe(
      "Credential Manager access was denied",
    );
    expect(screen.getByText("Credential store unavailable")).toBeTruthy();
    expect(screen.queryByText("Anonymous API access")).toBeNull();
  });

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

  it("keeps credential storage and cache cleanup actions explicit", async () => {
    const page = props();
    render(<SettingsPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Save securely" }));
    expect(page.onSaveGithubToken).toHaveBeenCalledWith("secret-value");
    fireEvent.click(screen.getByRole("button", { name: "Refresh sizes" }));
    await waitFor(() => expect(page.onRefreshStorage).toHaveBeenCalledOnce());
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    fireEvent.click(
      screen.getByRole("button", { name: "Clear compiler cache" }),
    );
    expect(confirm).toHaveBeenCalledOnce();
    await waitFor(() => expect(page.onClearCompilerCache).toHaveBeenCalledOnce());
  });

  it("confirms temporary community checkout cleanup using the measured size", async () => {
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
    await waitFor(() => expect(page.onClearCommunityWorktrees).toHaveBeenCalledOnce());
  });

  it("requires explicit confirmation before deleting session logs", async () => {
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
    await waitFor(() => expect(page.onClearSessionLogs).toHaveBeenCalledOnce());
  });

  it("confirms cleanup of measured incomplete installations", async () => {
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
    await waitFor(() => expect(page.onCleanIncompleteBuilds).toHaveBeenCalledOnce());
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
