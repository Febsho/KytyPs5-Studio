// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { CommunityPRPage } from "../types/studio";
import { useCommunityPRData } from "./useCommunityPRData";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const pageFor = (repository: string): CommunityPRPage => ({
  repository,
  items: [],
  total_count: 0,
  page: 1,
  per_page: 50,
  has_more: false,
  truncated: false,
  stale: false,
  refresh_error: null,
});

const mockedInvoke = vi.mocked(invoke);

beforeEach(() => {
  vi.useFakeTimers();
  mockedInvoke.mockReset();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useCommunityPRData", () => {
  it("debounces repository search and applies the latest search parameters", async () => {
    mockedInvoke.mockResolvedValue(pageFor("KytyPS5/KytyPS5"));
    const setMessage = vi.fn();
    const { result, rerender } = renderHook(
      ({ active }) => useCommunityPRData(active, 3, setMessage),
      { initialProps: { active: false } },
    );
    act(() => result.current.setSearch("vulkan"));
    rerender({ active: true });
    act(() => vi.advanceTimersByTime(349));
    expect(mockedInvoke).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(mockedInvoke).toHaveBeenCalledTimes(1);
    expect(mockedInvoke).toHaveBeenCalledWith("list_community_pull_requests", {
      repositoryId: 3,
      page: 1,
      state: "open",
      search: "vulkan",
    });
    expect(result.current.pullRequestPage?.repository).toBe("KytyPS5/KytyPS5");
    expect(result.current.loading).toBe(false);
  });

  it("discards late results from the previous repository", async () => {
    let resolveFirst!: (page: CommunityPRPage) => void;
    mockedInvoke
      .mockImplementationOnce(
        () => new Promise((resolve) => (resolveFirst = resolve)),
      )
      .mockResolvedValueOnce(pageFor("owner/second"));
    const setMessage = vi.fn();
    const { result, rerender } = renderHook(
      ({ repositoryId }) => useCommunityPRData(true, repositoryId, setMessage),
      { initialProps: { repositoryId: 1 as number | null } },
    );

    await act(async () => {
      vi.advanceTimersByTime(350);
      await Promise.resolve();
    });
    expect(result.current.loading).toBe(true);

    rerender({ repositoryId: 2 });
    await act(async () => {
      vi.advanceTimersByTime(350);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(result.current.pullRequestPage?.repository).toBe("owner/second");

    await act(async () => {
      resolveFirst(pageFor("owner/first"));
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(result.current.pullRequestPage?.repository).toBe("owner/second");
  });
});
