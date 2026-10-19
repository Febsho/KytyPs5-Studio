import { useCallback, useRef, useState } from "react";
import type { Build, Game, Repo, Session, System } from "../types/studio";

export type StudioCommand = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export function useStudioData(call: StudioCommand) {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [builds, setBuilds] = useState<Build[]>([]);
  const [games, setGames] = useState<Game[]>([]);
  const [system, setSystem] = useState<System | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const refreshRequest = useRef(0);

  const refresh = useCallback(async () => {
    const request = ++refreshRequest.current;
    const [nextRepos, nextBuilds, nextGames, nextSystem, nextSessions] =
      await Promise.all([
        call<Repo[]>("list_repos"),
        call<Build[]>("list_builds"),
        call<Game[]>("list_games"),
        call<System>("system_info"),
        call<Session[]>("recent_sessions"),
      ]);
    if (request !== refreshRequest.current) return;
    setRepos(nextRepos);
    setBuilds(nextBuilds);
    setGames(nextGames);
    setSystem(nextSystem);
    setSessions(nextSessions);
  }, [call]);
  const refreshBuilds = useCallback(async () => {
    setBuilds(await call<Build[]>("list_builds"));
  }, [call]);

  return {
    repos,
    builds,
    games,
    system,
    sessions,
    setSessions,
    refresh,
    refreshBuilds,
  };
}
