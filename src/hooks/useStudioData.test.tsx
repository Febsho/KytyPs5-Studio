// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useStudioData, type StudioCommand } from "./useStudioData";
import type { System } from "../types/studio";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

describe("useStudioData", () => {
  it("refreshes all shared backend data as one snapshot", async () => {
    const repo = { id: 1, owner: "KytyPS5", name: "KytyPS5" } as never;
    const build = { id: 2, repo_id: 1, commit_sha: "abc" } as never;
    const game = { id: 3, title: "Game" } as never;
    const session = { id: 4, game_id: 3 } as never;
    const system = { os: "linux" } as System;
    const values: Record<string, unknown> = {
      list_repos: [repo],
      list_builds: [build],
      list_games: [game],
      system_info: system,
      recent_sessions: [session],
    };
    const call = vi.fn(async <T,>(command: string) => values[command] as T);
    const { result } = renderHook(() => useStudioData(call as StudioCommand));

    await act(async () => result.current.refresh());

    expect(result.current.repos).toEqual([repo]);
    expect(result.current.builds).toEqual([build]);
    expect(result.current.games).toEqual([game]);
    expect(result.current.system).toEqual(system);
    expect(result.current.sessions).toEqual([session]);
    expect(call).toHaveBeenCalledTimes(5);
  });

  it("refreshes collections without rerunning system diagnostics", async () => {
    const values: Record<string, unknown> = {
      list_repos: [],
      list_builds: [],
      list_games: [],
      recent_sessions: [],
    };
    const call = vi.fn(async <T,>(command: string) => values[command] as T);
    const { result } = renderHook(() => useStudioData(call as StudioCommand));

    await act(async () => result.current.refreshData());

    expect(call).toHaveBeenCalledTimes(4);
    expect(call).not.toHaveBeenCalledWith("system_info");
    expect(result.current.system).toBeNull();
  });

  it("keeps the previous snapshot when one refresh request fails", async () => {
    const call = vi.fn(async <T,>(command: string) => {
      if (command === "list_repos") return [{ id: 1 }] as T;
      if (command === "list_builds") throw new Error("build list failed");
      return [] as T;
    });
    const { result } = renderHook(() => useStudioData(call as StudioCommand));

    await act(async () => {
      await expect(result.current.refresh()).rejects.toThrow("build list failed");
    });

    expect(result.current.repos).toEqual([]);
    expect(result.current.builds).toEqual([]);
  });

  it("does not let an older refresh overwrite a newer snapshot", async () => {
    const commands = [
      "list_repos",
      "list_builds",
      "list_games",
      "system_info",
      "recent_sessions",
    ];
    const values = new Map(
      commands.map((command) => [
        command,
        [deferred<unknown>(), deferred<unknown>()] as const,
      ]),
    );
    const callCounts = new Map<string, number>();
    const call = vi.fn(async <T,>(command: string) => {
      const index = callCounts.get(command) ?? 0;
      callCounts.set(command, index + 1);
      return values.get(command)![index].promise as Promise<T>;
    });
    const { result } = renderHook(() => useStudioData(call as StudioCommand));
    let older!: Promise<void>;
    let newer!: Promise<void>;

    await act(async () => {
      older = result.current.refresh();
      newer = result.current.refresh();
      commands.forEach((command) => values.get(command)![1].resolve(
        command === "list_repos" ? [{ id: 2 }] : [],
      ));
      await newer;
    });
    await act(async () => {
      commands.forEach((command) => values.get(command)![0].resolve(
        command === "list_repos" ? [{ id: 1 }] : [],
      ));
      await older;
    });

    expect(result.current.repos).toEqual([{ id: 2 }]);
  });
});
