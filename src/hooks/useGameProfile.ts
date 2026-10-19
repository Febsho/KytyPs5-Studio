import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Profile, ProfileCapabilities } from "../types/studio";

type ProfileLoader = (gameId: number) => Promise<Profile>;
type CapabilityLoader = (gameId: number) => Promise<ProfileCapabilities>;

const loadProfile: ProfileLoader = (gameId) =>
  invoke<Profile>("get_profile", { gameId });
const loadCapabilities: CapabilityLoader = (gameId) =>
  invoke<ProfileCapabilities>("get_profile_capabilities", { gameId });
const defaultLoaders = { loadProfile, loadCapabilities };

const emptyProfile = (): Profile => ({
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
});

export function useGameProfile(
  gameId: number | undefined,
  buildIdentity: string,
  onError: (message: string) => void,
  loaders: { loadProfile: ProfileLoader; loadCapabilities: CapabilityLoader } =
    defaultLoaders,
) {
  const [profile, setProfile] = useState<Profile>(emptyProfile);
  const [capabilities, setCapabilities] =
    useState<ProfileCapabilities | null>(null);
  const [savedSnapshot, setSavedSnapshot] = useState("");
  const [loading, setLoading] = useState(false);
  const [loadedIdentity, setLoadedIdentity] = useState<string | null>(null);
  const requestId = useRef(0);
  const onErrorRef = useRef(onError);
  const loadersRef = useRef(loaders);
  onErrorRef.current = onError;
  loadersRef.current = loaders;
  const identity = gameId === undefined ? null : `${gameId}:${buildIdentity}`;

  useEffect(() => {
    const request = ++requestId.current;
    setLoadedIdentity(null);
    setCapabilities(null);
    setSavedSnapshot("");
    setProfile(emptyProfile());
    setLoading(gameId !== undefined);
    if (gameId === undefined) return;

    Promise.all([
      loadersRef.current.loadProfile(gameId),
      loadersRef.current.loadCapabilities(gameId),
    ])
      .then(([loaded, available]) => {
        if (request !== requestId.current) return;
        setProfile(loaded);
        setSavedSnapshot(JSON.stringify(loaded));
        setCapabilities(available);
        setLoadedIdentity(identity);
      })
      .catch((error: unknown) => {
        if (request === requestId.current) onErrorRef.current(String(error));
      })
      .finally(() => {
        if (request === requestId.current) setLoading(false);
      });

    return () => {
      if (requestId.current === request) requestId.current++;
    };
  }, [gameId, buildIdentity, identity]);

  const copyFromGame = useCallback(
    async (sourceGameId: number) => {
      const request = ++requestId.current;
      setLoading(true);
      try {
        const source = await loadersRef.current.loadProfile(sourceGameId);
        if (request !== requestId.current) return false;
        setSavedSnapshot(JSON.stringify(profile));
        setProfile(source);
        return true;
      } catch (error) {
        if (request === requestId.current)
          onErrorRef.current(`Could not copy game profile: ${String(error)}`);
        return false;
      } finally {
        if (request === requestId.current) setLoading(false);
      }
    },
    [profile],
  );

  const restore = useCallback(() => {
    if (!savedSnapshot) return;
    try {
      setProfile(JSON.parse(savedSnapshot) as Profile);
    } catch {
      onErrorRef.current("Could not restore the saved game profile snapshot");
    }
  }, [savedSnapshot]);

  const ready = identity !== null && loadedIdentity === identity;
  return {
    profile: ready ? profile : emptyProfile(),
    setProfile,
    capabilities: ready ? capabilities : null,
    savedSnapshot: ready ? savedSnapshot : "",
    setSavedSnapshot,
    loading: identity !== null && (!ready || loading),
    setLoading,
    copyFromGame,
    restore,
  };
}
