// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Game, Profile, ProfileCapabilities } from "../../types/studio";
import { GameProfileEditor } from "./GameProfileEditor";

afterEach(cleanup);

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

const game = (id: number, title: string): Game => ({
  id,
  path: `/games/${id}`,
  title,
  favorite: false,
  build_id: null,
  play_seconds: 0,
  last_played: null,
  title_id: null,
  game_version: null,
  icon_path: null,
  install_size: null,
  file_status: "Available",
});
const profile: Profile = {
  screen_width: null,
  screen_height: null,
  present_mode: null,
  perf_overlay: false,
  fullscreen: false,
  controller_color: null,
  controller_volume: null,
  controller_vibration: null,
  keyboard_player: null,
  options: {},
};
const capabilities: ProfileCapabilities = {
  screen_width: false,
  screen_height: false,
  present_mode: false,
  perf_overlay: false,
  fullscreen: true,
  controller_color: false,
  controller_volume: false,
  controller_vibration: false,
  keyboard_player: false,
  options: ["--fullscreen"],
};

describe("GameProfileEditor", () => {
  it("forwards copy, clear, save, and reset actions", () => {
    const actions = {
      setProfile: vi.fn(),
      setSourceGameId: vi.fn(),
      onCopy: vi.fn(),
      onRestore: vi.fn(),
      onClearUnsupported: vi.fn(),
      onSave: vi.fn(),
      onReset: vi.fn(),
    };
    render(
      <GameProfileEditor
        gameId={1}
        games={[game(1, "Current"), game(2, "Source")]}
        profile={profile}
        capabilities={capabilities}
        globalOptions={{}}
        gpuDevices={[]}
        sourceGameId={2}
        loading={false}
        dirty={false}
        canRestore={false}
        unsupported
        {...actions}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Copy into editor" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear unsupported saved options" }));
    fireEvent.click(screen.getByRole("button", { name: "Save profile" }));
    fireEvent.click(screen.getByRole("button", { name: "Reset game profile" }));

    expect(actions.onCopy).toHaveBeenCalledWith(2);
    expect(actions.onClearUnsupported).toHaveBeenCalledOnce();
    expect(actions.onSave).toHaveBeenCalledOnce();
    expect(actions.onReset).toHaveBeenCalledOnce();
  });

  it("prevents saving or resetting a profile before its load finishes", () => {
    render(
      <GameProfileEditor
        gameId={1}
        games={[game(1, "Current")]}
        profile={profile}
        setProfile={vi.fn()}
        capabilities={null}
        globalOptions={{}}
        gpuDevices={[]}
        sourceGameId={null}
        setSourceGameId={vi.fn()}
        loading
        dirty={false}
        canRestore={false}
        unsupported={false}
        onCopy={vi.fn()}
        onRestore={vi.fn()}
        onClearUnsupported={vi.fn()}
        onSave={vi.fn()}
        onReset={vi.fn()}
      />,
    );

    expect(
      (screen.getByRole("button", { name: "Loading profile…" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      (screen.getByRole("button", { name: "Reset game profile" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
