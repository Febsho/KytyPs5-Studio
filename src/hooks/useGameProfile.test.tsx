// @vitest-environment jsdom
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Profile, ProfileCapabilities } from "../types/studio";
import { useGameProfile } from "./useGameProfile";

const available: ProfileCapabilities = {
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
const profile = (value: string): Profile => ({
  screen_width: null,
  screen_height: null,
  present_mode: null,
  perf_overlay: false,
  fullscreen: false,
  controller_color: null,
  controller_volume: null,
  controller_vibration: null,
  keyboard_player: null,
  options: { "--user-name": value },
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => (resolve = res));
  return { promise, resolve };
}

describe("useGameProfile", () => {
  it("ignores stale profile and capability responses after switching games", async () => {
    const oldProfile = deferred<Profile>();
    const oldCapabilities = deferred<ProfileCapabilities>();
    const newProfile = deferred<Profile>();
    const newCapabilities = deferred<ProfileCapabilities>();
    const loaders = {
      loadProfile: (gameId: number) =>
        gameId === 1 ? oldProfile.promise : newProfile.promise,
      loadCapabilities: (gameId: number) =>
        gameId === 1 ? oldCapabilities.promise : newCapabilities.promise,
    };
    const onError = vi.fn();
    const { result, rerender } = renderHook(
      ({ gameId }) => useGameProfile(gameId, "build-a", onError, loaders),
      { initialProps: { gameId: 1 as number | undefined } },
    );

    rerender({ gameId: 2 });
    await act(async () => {
      newProfile.resolve(profile("game two"));
      newCapabilities.resolve(available);
      await Promise.resolve();
    });
    await waitFor(() => expect(result.current.loading).toBe(false));

    rerender({ gameId: 1 });
    expect(result.current.loading).toBe(true);
    expect(result.current.profile.options).toEqual({});
    expect(result.current.capabilities).toBeNull();

    rerender({ gameId: 2 });
    await act(async () => {
      oldProfile.resolve(profile("stale game one"));
      oldCapabilities.resolve(available);
      await Promise.resolve();
    });

    expect(result.current.profile.options["--user-name"]).toBe("game two");
    expect(result.current.savedSnapshot).toBe(JSON.stringify(profile("game two")));
    expect(result.current.capabilities).toEqual(available);
    expect(onError).not.toHaveBeenCalled();
  });

  it("reloads profile capability state when the selected build changes", async () => {
    let activeName = "build a";
    const loaders = {
      loadProfile: async () => profile(activeName),
      loadCapabilities: async () => available,
    };
    const onError = vi.fn();
    const { result, rerender } = renderHook(
      ({ buildIdentity }) =>
        useGameProfile(7, buildIdentity, onError, loaders),
      { initialProps: { buildIdentity: "build-a" } },
    );
    await waitFor(() =>
      expect(result.current.profile.options["--user-name"]).toBe("build a"),
    );
    activeName = "build b";

    rerender({ buildIdentity: "build-b" });
    expect(result.current.loading).toBe(true);
    expect(result.current.profile.options).toEqual({});
    expect(result.current.capabilities).toBeNull();
    await waitFor(() =>
      expect(result.current.profile.options["--user-name"]).toBe("build b"),
    );
  });
});
