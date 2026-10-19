import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommunityPRPage } from "../types/studio";
import type { Dispatch, SetStateAction } from "react";
import {
  filterAndSortCommunityPRs,
  type CommunityPRSort,
  type CommunityPRState,
} from "../components/community/communityModel";

export function useCommunityPRData(
  active: boolean,
  repositoryId: number | null,
  setMessage: Dispatch<SetStateAction<string>>,
) {
  const [state, setState] = useState<CommunityPRState>("open");
  const [mergedOnly, setMergedOnly] = useState(false);
  const [page, setPage] = useState(1);
  const [refreshCount, setRefreshCount] = useState(0);
  const [pullRequestPage, setPullRequestPage] =
    useState<CommunityPRPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [number, setNumber] = useState("");
  const [label, setLabel] = useState("");
  const [sort, setSort] = useState<CommunityPRSort>("updated");
  const previousRepository = useRef<number | null>(repositoryId);

  useEffect(() => {
    if (previousRepository.current !== repositoryId) {
      previousRepository.current = repositoryId;
      setPullRequestPage(null);
    }
    if (!active || !repositoryId) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(false);
    const timer = window.setTimeout(() => {
      if (cancelled) return;
      setLoading(true);
      invoke<CommunityPRPage>("list_community_pull_requests", {
        repositoryId,
        page,
        state,
        search,
      })
        .then((result) => {
          if (!cancelled) {
            setPullRequestPage(result);
            if (result.refresh_error) setMessage(result.refresh_error);
          }
        })
        .catch((error) => {
          if (!cancelled) {
            setPullRequestPage(null);
            setMessage(`Pull requests could not be loaded: ${String(error)}`);
          }
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    }, 350);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [active, repositoryId, page, state, refreshCount, search, setMessage]);

  const visiblePullRequests = useMemo(
    () =>
      filterAndSortCommunityPRs(pullRequestPage?.items ?? [], {
        search,
        number,
        label,
        mergedOnly,
        sort,
      }),
    [pullRequestPage, search, number, label, mergedOnly, sort],
  );

  const refresh = useCallback(() => setRefreshCount((count) => count + 1), []);

  return {
    state,
    setState,
    mergedOnly,
    setMergedOnly,
    page,
    setPage,
    pullRequestPage,
    loading,
    search,
    setSearch,
    number,
    setNumber,
    label,
    setLabel,
    sort,
    setSort,
    visiblePullRequests,
    refresh,
  };
}
