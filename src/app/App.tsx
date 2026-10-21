import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  filterAndSortGames,
  isPubliclyRecognizedGame,
} from "../components/games/libraryModel";
import {
  Activity,
  BookOpen,
  CircleAlert,
  Code2,
  Gamepad2,
  GitBranch,
  Home,
  Library,
  Maximize2,
  Minus,
  RefreshCw,
  Settings,
  GitPullRequest,
  X,
} from "lucide-react";
import type {
  Repo,
  StorageReport,
  BuildSourceInfo,
  Game,
  GameCompatibility,
  OfficialCompatibilityDetails,
  OfficialCompatibilityIndex,
  ManualCompatibilityObservation,
  AppUpdateInfo,
  Fork,
  GithubDetails,
  GraphicsDiagnostics,
  ProfileCapabilities,
  Session,
  ProcessMetric,
  Job,
  GameScanEvent,
  GameScanResult,
  GameContent,
  RemoteCheatFile,
  CommunityPR,
  CommunityPRDetails,
  CommunityBuildHistory as CommunityBuildHistoryEntry,
} from "../types/studio";
import {
  loadGameIcon,
  loadGameBackground,
  retainGameIcon,
  retainGameBackground,
} from "../services/gameArtwork";
import { formatBytes, short } from "../utils/format";
import { createJobEventGate } from "../utils/jobEventGate";
import { currentCompatibilityOS } from "../utils/compatibility";

import { HomePage } from "../pages/HomePage";
import { DiagnosticsPage } from "../pages/DiagnosticsPage";
import { GameDetailsPage } from "../pages/GameDetailsPage";
import { BuildStudioPage } from "../pages/BuildStudioPage";
import { EmulatorManagerPage } from "../pages/EmulatorManagerPage";
import { GameLibraryPage } from "../pages/GameLibraryPage";
import { SteamShortcutDialog } from "../components/games/SteamShortcutDialog";
import { SettingsPage } from "../pages/SettingsPage";
import { LauncherSessionModal } from "../components/emulator/LauncherSessionModal";
import { RepositoryManagerPage } from "../pages/RepositoryManagerPage";
import { GameProfileEditor } from "../components/settings/GameProfileEditor";
import { profileWithOptions } from "../utils/profile";
import { MainNavigation } from "../components/navigation/MainNavigation";
import { CommunityPullRequestDrawer } from "../components/community/CommunityPullRequestDrawer";
import { CommunityBuildHistory } from "../components/community/CommunityBuildHistory";
import { CommunityPRFilters } from "../components/community/CommunityPRFilters";
import { CommunityPRTable } from "../components/community/CommunityPRTable";
import { CommunityPRSelection } from "../components/community/CommunityPRSelection";
import { ControllerRemapDialog } from "../components/settings/ControllerRemapDialog";
import { useCommunityPRData } from "../hooks/useCommunityPRData";
import { useGameProfile } from "../hooks/useGameProfile";
import { useStudioData } from "../hooks/useStudioData";
import {
  refreshProcessMetricsNow,
  useProcessMetrics,
} from "../hooks/useProcessMetrics";
import {
  mappedControlPressed,
  readControllerMapping,
  type ControllerMapping,
} from "../utils/controllerMapping";

const nav = [
  { id: "home", label: "Home", icon: Home },
  { id: "library", label: "Library", icon: Library },
  { id: "emulators", label: "Emulators", icon: Gamepad2 },
  { id: "forks", label: "Fork Manager", icon: GitBranch },
  { id: "community", label: "Community Builds", icon: GitPullRequest },
  { id: "build", label: "Build Studio", icon: Code2 },
  { id: "diagnostics", label: "Diagnostics", icon: Activity },
  { id: "settings", label: "Settings", icon: Settings },
] as const;
type SessionPerformanceSamples = {
  cpuTotal: number;
  cpuCount: number;
  peakCpu: number | null;
  peakResidentBytes: number | null;
};
const readUiState = (key: string) => {
  try {
    return localStorage.getItem(`studio.ui.${key}`);
  } catch {
    return null;
  }
};
const readUiId = (key: string) => {
  const value = Number(readUiState(key));
  return Number.isSafeInteger(value) && value > 0 ? value : null;
};
export function App() {
  const [page, setPage] = useState<(typeof nav)[number]["id"] | "game-details">(
    () => {
      const saved = readUiState("page");
      return saved === "game-details"
        ? "game-details"
        : nav.some((item) => item.id === saved)
          ? (saved as (typeof nav)[number]["id"])
          : "home";
    },
  );
  const [dragActive, setDragActive] = useState(false);
  const [steamGame, setSteamGame] = useState<Game | null>(null);
  const [steamBuildId, setSteamBuildId] = useState<number | null>(null);
  const [steamBusy, setSteamBusy] = useState(false);
  const [controllerRemapOpen, setControllerRemapOpen] = useState(false);
  const controllerRemappingEnabled = false;
  const controllerRemapOpenRef = useRef(false);
  controllerRemapOpenRef.current = controllerRemapOpen;
  const [controllerMapping, setControllerMapping] = useState<ControllerMapping>(
    () => readControllerMapping(readUiState("controller_mapping")),
  );
  const controllerMappingRef = useRef(controllerMapping);
  controllerMappingRef.current = controllerMapping;
  const [gameScan, setGameScan] = useState<GameScanEvent | null>(null);
  const [gameCompatibility, setGameCompatibility] =
    useState<GameCompatibility | null>(null);
  const [officialCompatibilityIndex, setOfficialCompatibilityIndex] =
    useState<OfficialCompatibilityIndex | null>(null);
  const [officialCompatibilityDetails, setOfficialCompatibilityDetails] =
    useState<OfficialCompatibilityDetails | null>(null);
  const [officialCompatibilityError, setOfficialCompatibilityError] = useState<string | null>(null);
  const [compatibilityObservations, setCompatibilityObservations] = useState<
    ManualCompatibilityObservation[]
  >([]);
  const [manualOutcome, setManualOutcome] = useState("reached_menu");
  const [manualNotes, setManualNotes] = useState("");
  const [forks, setForks] = useState<Fork[]>([]);
  const [storageReport, setStorageReport] = useState<StorageReport | null>(
    null,
  );
  const [appUpdateInfo, setAppUpdateInfo] = useState<AppUpdateInfo | null>(null);
  const [appUpdateStatus, setAppUpdateStatus] = useState("Checking for updates when Studio starts…");
  const [appUpdateBusy, setAppUpdateBusy] = useState(false);
  const [importMenuOpen, setImportMenuOpen] = useState(false);
  const importMenuRef = useRef<HTMLDivElement>(null);
  const appUpdateInFlight = useRef(false);
  const repoUpdateCheckInFlight = useRef(false);
  const pendingRepoUpdateCheck = useRef<(() => void) | null>(null);
  const [graphics, setGraphics] = useState<GraphicsDiagnostics | null>(null);
  const [repoUrl, setRepoUrl] = useState("");
  const [selectedRepo, setSelectedRepo] = useState<number | null>(() =>
    readUiId("repository"),
  );
  const [buildSourceInfo, setBuildSourceInfo] =
    useState<BuildSourceInfo | null>(null);
  const [githubDetails, setGithubDetails] = useState<GithubDetails | null>(
    null,
  );
  const [selectedGame, setSelectedGame] = useState<number | null>(() =>
    readUiId("game"),
  );
  const [gameDetailsTab, setGameDetailsTab] = useState<
    "Overview" | "Settings" | "Compatibility" | "Logs" | "Trophies" | "Cheats"
  >("Overview");
  const [homeSelection, setHomeSelection] = useState<number | null>(() =>
    readUiId("home_game"),
  );
  const [specsOpen, setSpecsOpen] = useState(
    () => readUiState("specs") === "open",
  );
  const [buildMode, setBuildMode] = useState<"simple" | "advanced">(() =>
    readUiState("build_mode") === "advanced" ? "advanced" : "simple",
  );
  const [launcherDismissed, setLauncherDismissed] = useState(false);
  const [launcherSessionId, setLauncherSessionId] = useState<number | null>(
    null,
  );
  const [buildType, setBuildType] = useState("Release");
  const [cleanBuild, setCleanBuild] = useState(false);
  const [activateAfterBuild, setActivateAfterBuild] = useState(true);
  const [jobs, setJobs] = useState(
    Math.max(1, Math.min(128, (navigator.hardwareConcurrency || 4) - 1)),
  );
  const [logs, setLogs] = useState<string[]>([]);
  const pendingJobLogs = useRef<string[]>([]);
  const logFlushScheduled = useRef(false);
  const [busy, setBusy] = useState(false);
  const jobStartInFlight = useRef(false);
  const [currentBuildPhase, setCurrentBuildPhase] = useState("Idle");
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const jobStartedAt = useRef<number | null>(null);
  const jobEventGate = useRef(createJobEventGate<Job>());
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!message) return;
    const timeout = window.setTimeout(() => {
      setMessage((current) => (current === message ? "" : current));
    }, 5000);
    return () => window.clearTimeout(timeout);
  }, [message]);
  const [query, setQuery] = useState(() => readUiState("query") || "");
  const [libraryView, setLibraryView] = useState<"grid" | "list">(() =>
    readUiState("library_view") === "list" ? "list" : "grid",
  );
  const [libraryFilter, setLibraryFilter] = useState<
    "all" | "favorites" | "recent"
  >(() =>
    readUiState("library_filter") === "favorites"
      ? "favorites"
      : readUiState("library_filter") === "recent"
        ? "recent"
        : "all",
  );
  const [librarySort, setLibrarySort] = useState<
    "title" | "recent" | "playtime" | "size"
  >(() =>
    ["recent", "playtime", "size"].includes(readUiState("library_sort") || "")
      ? (readUiState("library_sort") as "recent" | "playtime" | "size")
      : "title",
  );
  const [selectedLog, setSelectedLog] = useState("");
  const [selectedLogPath, setSelectedLogPath] = useState<string | null>(null);
  const [profileCopySource, setProfileCopySource] = useState<number | null>(
    null,
  );
  const [globalProfileOptions, setGlobalProfileOptions] = useState<
    Record<string, string>
  >({});
  const [globalProfileCapabilities, setGlobalProfileCapabilities] =
    useState<ProfileCapabilities | null>(null);
  const [gameIcon, setGameIcon] = useState<string | null>(null);
  const [gameBackground, setGameBackground] = useState<string | null>(null);
  const [featuredBackground, setFeaturedBackground] = useState<string | null>(
    null,
  );
  const [updateInterval, setUpdateInterval] = useState("24");
  const [discordPresenceEnabled, setDiscordPresenceEnabled] = useState(true);
  const [processMetrics, setProcessMetrics] = useState<ProcessMetric[]>([]);
  const processMetricsRef = useRef<ProcessMetric[]>([]);
  const gamepadWindowFocused = useRef(true);
  const gamepadGameRunning = useRef(false);
  const sessionPerformanceSamples = useRef(
    new Map<number, SessionPerformanceSamples>(),
  );
  const [processMetricsReady, setProcessMetricsReady] = useState(false);
  const [autoUpdateMode, setAutoUpdateMode] = useState<"notify" | "build">(
    "notify",
  );
  const attemptedAutomaticUpdates = useRef(new Set<string>());
  const attemptedBuildValidation = useRef(new Set<number>());
  const notifiedUpdates = useRef(new Set<string>());
  const [logFilter, setLogFilter] = useState<
    "All" | "Info" | "Warning" | "Error" | "Debug"
  >("All");
  const [selectedPrs, setSelectedPrs] = useState<CommunityPR[]>([]);
  const [communityDetails, setCommunityDetails] =
    useState<CommunityPRDetails | null>(null);
  const [githubTokenAvailable, setGithubTokenAvailable] = useState(false);
  const [githubTokenError, setGithubTokenError] = useState<string | null>(null);
  const [githubTokenInput, setGithubTokenInput] = useState("");
  const [communityHistory, setCommunityHistory] = useState<
    CommunityBuildHistoryEntry[]
  >([]);
  const [communityPinnedHeads, setCommunityPinnedHeads] = useState<
    Record<number, string>
  >({});
  const [selectionReview, setSelectionReview] = useState<CommunityPRDetails[]>(
    [],
  );
  const [reviewLoading, setReviewLoading] = useState(false);
  const communityData = useCommunityPRData(
    page === "community",
    selectedRepo,
    setMessage,
  );
  const {
    state: communityState,
    setState: setCommunityState,
    mergedOnly: communityMergedOnly,
    setMergedOnly: setCommunityMergedOnly,
    page: communityPage,
    setPage: setCommunityPage,
    pullRequestPage: communityPrs,
    loading: communityLoading,
    search: communitySearch,
    setSearch: setCommunitySearch,
    number: communityNumber,
    setNumber: setCommunityNumber,
    label: communityLabel,
    setLabel: setCommunityLabel,
    sort: communitySort,
    setSort: setCommunitySort,
    visiblePullRequests: communityVisible,
    refresh: refreshCommunityPRs,
  } = communityData;
  const call = useCallback(async <T,>(
    cmd: string,
    args?: Record<string, unknown>,
  ): Promise<T> => {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      setMessage(String(e));
      throw e;
    }
  }, []);
  const {
    repos,
    builds,
    games,
    system,
    sessions,
    setSessions,
    refreshData,
    refreshSystem,
    refreshBuilds,
  } = useStudioData(call);
  useEffect(() => {
    if (!system?.memory_total_bytes) return;
    const cpuLimit = Math.max(1, (navigator.hardwareConcurrency || 4) - 1);
    const memoryGiB = system.memory_total_bytes / (1024 ** 3);
    const memoryLimit = Math.max(1, Math.floor(memoryGiB / 3));
    const recommended = Math.min(cpuLimit, memoryLimit);
    setJobs((current) => Math.min(current, recommended));
  }, [system?.memory_total_bytes]);
  useEffect(() => {
    let disposed = false;
    const refreshCompatibilityIndex = () => {
      void invoke<OfficialCompatibilityIndex>("official_compatibility_index")
        .then((result) => { if (!disposed) setOfficialCompatibilityIndex(result); })
        .catch(() => { if (!disposed) setOfficialCompatibilityIndex({ available: false, updated_at: null, error: "The official compatibility database could not be reached and no offline cache is available.", games: [] }); });
    };
    refreshCompatibilityIndex();
    const timer = window.setInterval(refreshCompatibilityIndex, 6 * 60 * 60 * 1000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, []);
  const trackedRepos = useMemo(() => repos.filter((repo) => !repo.removed), [repos]);
  const currentRepo = trackedRepos.find((r) => r.id === selectedRepo) || trackedRepos[0];
  useEffect(() => {
    if (page !== "emulators") return;
    void refreshBuilds().catch(() => {});
    const timer = window.setInterval(() => {
      void refreshBuilds().catch(() => {});
    }, 30_000);
    return () => window.clearInterval(timer);
  }, [page, refreshBuilds]);
  const exportSelectedLog = async () => {
    if (!selectedLogPath) return;
    const destination = await save({
      defaultPath: "kytyps5-session.log",
      filters: [{ name: "Log files", extensions: ["log", "txt"] }],
    });
    if (destination)
      await call("export_log", { path: selectedLogPath, destination });
  };
  const inspectGameContent = useCallback(
    (gameId: number) => call<GameContent>("inspect_game_content", { gameId }),
    [call],
  );
  const fetchGameCheats = useCallback(
    (gameId: number) => call<RemoteCheatFile[]>("fetch_game_cheats", { gameId }),
    [call],
  );
  const downloadGameCheat = useCallback(
    (gameId: number, filename: string) =>
      call<string>("download_game_cheat", { gameId, filename }),
    [call],
  );
  const importCustomGamePatch = useCallback(async (gameId: number) => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "GoldHEN JSON patch", extensions: ["json"] }],
    });
    if (typeof selected !== "string") return null;
    return call<{ path: string; content: string }>("import_game_patch", { gameId, path: selected });
  }, [call]);
  const saveCustomGamePatch = useCallback(
    (gameId: number, content: string) =>
      call<string>("save_custom_game_patch", { gameId, content }),
    [call],
  );
  useEffect(() => {
    for (const [key, value] of [
      ["page", page],
      ["repository", selectedRepo?.toString() || ""],
      ["game", selectedGame?.toString() || ""],
      ["query", query],
      ["library_view", libraryView],
      ["library_filter", libraryFilter],
      ["library_sort", librarySort],
      ["home_game", homeSelection?.toString() || ""],
      ["build_mode", buildMode],
      ["specs", specsOpen ? "open" : "closed"],
    ] as const) {
      try {
        localStorage.setItem(`studio.ui.${key}`, value);
      } catch {
        break;
      }
    }
  }, [
    page,
    selectedRepo,
    selectedGame,
    query,
    libraryView,
    libraryFilter,
    librarySort,
    homeSelection,
    buildMode,
    specsOpen,
  ]);
  useEffect(() => {
    if (!trackedRepos.length) {
      setSelectedRepo(null);
      return;
    }
    if (
      selectedRepo === null ||
      !trackedRepos.some((repo) => repo.id === selectedRepo)
    )
      setSelectedRepo(
        trackedRepos.find((repo) => repo.source_path)?.id ?? trackedRepos[0].id,
      );
  }, [trackedRepos, selectedRepo]);
  useEffect(() => {
    if (page !== "settings") return;
    let cancelled = false;
    invoke<StorageReport>("storage_report")
      .then((report) => {
        if (!cancelled) setStorageReport(report);
      })
      .catch((error) => {
        if (!cancelled)
          setMessage(`Storage usage unavailable: ${String(error)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [page, builds.length]);
  useEffect(() => {
    if (!builds.length) return;
    const pending = [...builds]
      .filter(
        (build) =>
          build.validation_status === "not-checked" &&
          !attemptedBuildValidation.current.has(build.id),
      )
      .sort((a, b) => Number(b.active) - Number(a.active));
    if (!pending.length) return;
    let disposed = false;
    for (const build of pending) attemptedBuildValidation.current.add(build.id);
    void (async () => {
      const failures: string[] = [];
      for (const build of pending) {
        if (disposed) return;
        try {
          await invoke<string>("validate_build", { id: build.id });
        } catch (error) {
          failures.push(`${short(build.commit_sha)}: ${String(error)}`);
        }
      }
      if (!disposed) {
        if (failures.length)
          setMessage(`Build validation failed — ${failures.join("; ")}`);
        refreshData().catch(() => {});
      }
    })();
    return () => {
      disposed = true;
    };
  }, [builds]);
  useEffect(() => {
    let stop: (() => void) | undefined;
    listen<GameScanEvent>("studio-game-scan", (event) => {
      setGameScan(event.payload);
      if (event.payload.done) refreshData().catch(() => {});
    })
      .then((unlisten) => {
        stop = unlisten;
      })
      .catch((error) =>
        setMessage(`Game scan progress unavailable: ${String(error)}`),
      );
    return () => stop?.();
  }, []);
  const queueJobLog = (line: string) => {
    pendingJobLogs.current.push(line);
    if (logFlushScheduled.current) return;
    logFlushScheduled.current = true;
    window.requestAnimationFrame(() => {
      logFlushScheduled.current = false;
      const next = pendingJobLogs.current.splice(0);
      if (next.length)
        setLogs((old) => [...old, ...next].slice(-800));
    });
  };
  const handleJobEvent = (j: Job) => {
    const isJob =
      j.kind === "build" ||
      j.kind === "clone" ||
      j.kind === "community-build" ||
      j.kind === "install-tools";
    if (isJob) queueJobLog(`[${j.channel} · ${j.phase}] ${j.line}`);
    if ((j.kind === "build" || j.kind === "clone" || j.kind === "community-build" || j.kind === "install-tools") && !j.done)
      setCurrentBuildPhase(j.phase);
    if (j.done) {
      if (isJob) {
        setBusy(false);
        jobEventGate.current.complete(j.job_id);
        if (j.kind === "build" || j.kind === "clone" || j.kind === "community-build" || j.kind === "install-tools")
          setCurrentBuildPhase(
            j.phase === "Cancelled"
              ? "Cancelled"
              : j.success
                ? "Completed"
                : "Failed",
          );
        if (jobStartedAt.current !== null)
          setElapsedSeconds(
            Math.floor((Date.now() - jobStartedAt.current) / 1000),
          );
        jobStartedAt.current = null;
      }
      setMessage(j.line);
      if (
        j.kind === "build" ||
        j.kind === "community-build" ||
        j.kind === "game"
      ) {
        void invoke("desktop_notification", {
          title:
            j.kind === "build" || j.kind === "community-build"
              ? j.phase === "Cancelled"
                ? "Build cancelled"
                : j.success
                  ? "Build completed"
                  : "Build failed"
              : j.success
                ? "Emulator session ended"
                : "Emulator session error",
          body: j.line,
        }).catch(() => {});
      }
      refreshData().catch(() => {});
      if (j.kind === "install-tools") refreshSystem().catch(() => {});
    }
  };
  const activateJob = (jobId: number) => {
    jobEventGate.current.activate(jobId).forEach(handleJobEvent);
  };
  useEffect(() => {
    refreshData().catch(() => {});
    let stop = () => {};
    listen<Job>("studio-job", (e) => {
      const j = e.payload;
      const isJob =
        j.kind === "build" ||
        j.kind === "clone" ||
        j.kind === "community-build" ||
        j.kind === "install-tools";
      if (isJob) jobEventGate.current.receive(j).forEach(handleJobEvent);
      else handleJobEvent(j);
    }).then((f) => {
      stop = f;
    });
    return () => stop();
  }, []);
  useEffect(() => {
    if (page === "build" || page === "diagnostics")
      void refreshSystem().catch(() => {});
  }, [page, refreshSystem]);
  useEffect(() => {
    if (!busy) return;
    const tick = () => {
      if (jobStartedAt.current !== null)
        setElapsedSeconds(
          Math.floor((Date.now() - jobStartedAt.current) / 1000),
        );
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => window.clearInterval(timer);
  }, [busy]);
  useEffect(() => {
    invoke<string | null>("get_setting", { key: "update_interval" })
      .then((v) =>
        setUpdateInterval(
          ["0", "6", "24", "168"].includes(v || "") ? v! : "24",
        ),
      )
      .catch(() => setUpdateInterval("24"));
    invoke<string | null>("get_setting", { key: "auto_update_mode" })
      .then((v) => setAutoUpdateMode(v === "build" ? "build" : "notify"))
      .catch(() => setAutoUpdateMode("notify"));
    invoke<string | null>("get_setting", { key: "discord_presence_enabled" })
      .then((v) => setDiscordPresenceEnabled(v !== "false"))
      .catch(() => setDiscordPresenceEnabled(true));
  }, []);
  useEffect(() => {
    setGithubDetails(null);
  }, [selectedRepo]);
  useEffect(() => {
    if (page !== "build" || !currentRepo?.source_path) {
      setBuildSourceInfo(null);
      return;
    }
    let cancelled = false;
    setBuildSourceInfo(null);
    invoke<BuildSourceInfo>("inspect_build_source", {
      repositoryId: currentRepo.id,
      buildType,
    })
      .then((info) => {
        if (!cancelled) setBuildSourceInfo(info);
      })
      .catch((error) => {
        if (!cancelled)
          setMessage(`Build source inspection unavailable: ${String(error)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [page, currentRepo?.id, currentRepo?.source_path, buildType]);
  useEffect(() => {
    if (page !== "community") return;
    invoke<CommunityBuildHistoryEntry[]>("community_build_history")
      .then(setCommunityHistory)
      .catch(() => {});
  }, [page, busy]);
  useEffect(() => {
    invoke<boolean>("github_token_status")
      .then((available) => {
        setGithubTokenAvailable(available);
        setGithubTokenError(null);
      })
      .catch((error) => {
        setGithubTokenAvailable(false);
        setGithubTokenError(String(error));
      });
  }, []);
  const reviewCommunitySelection = async () => {
    if (!selectedRepo || !selectedPrs.length) return;
    setReviewLoading(true);
    setSelectionReview([]);
    try {
      const details = await Promise.all(
        selectedPrs.map((pr) =>
          invoke<CommunityPRDetails>("community_pull_request_details", {
            repositoryId: selectedRepo,
            number: pr.number,
          }),
        ),
      );
      if (
        details.some(
          (detail, index) =>
            detail.pull_request.head_sha !== selectedPrs[index].head_sha,
        )
      ) {
        setMessage(
          "A selected PR changed while it was being reviewed. Refresh and review the new head before building.",
        );
        return;
      }
      setSelectionReview(details);
    } catch (error) {
      setMessage(`Cannot review the full PR combination: ${String(error)}`);
    } finally {
      setReviewLoading(false);
    }
  };
  const reviewHistoryHeads = async (entry: CommunityBuildHistoryEntry) => {
    const repository = repos.find(
      (repo) => `${repo.owner}/${repo.name}` === entry.repository,
    );
    if (!repository) {
      setMessage(
        `Repository ${entry.repository} is no longer tracked; add and clone it before reviewing this build.`,
      );
      return;
    }
    setSelectedRepo(repository.id);
    setCommunityState("all");
    setCommunityMergedOnly(false);
    setCommunityPage(1);
    setBuildType(entry.build_type);
    setJobs(entry.jobs);
    setCleanBuild(entry.clean);
    setPage("community");
    setSelectedPrs([]);
    setSelectionReview([]);
    setCommunityPinnedHeads(Object.fromEntries(entry.pull_requests));
    setReviewLoading(true);
    try {
      const details = await Promise.all(
        entry.pull_requests.map(([number]) =>
          invoke<CommunityPRDetails>("community_pull_request_details", {
            repositoryId: repository.id,
            number,
          }),
        ),
      );
      setSelectedPrs(details.map((detail) => detail.pull_request));
      setSelectionReview(details);
      setMessage(
        "Configuration loaded with its saved build settings. Review current PR heads before rebuilding or duplicating it; the previous verified build remains installed.",
      );
    } catch (error) {
      setCommunityPinnedHeads({});
      setMessage(
        `Could not refresh this build configuration: ${String(error)}`,
      );
    } finally {
      setReviewLoading(false);
    }
  };
  const handleProcessMetrics = useCallback((metrics: ProcessMetric[]) => {
    const wasActive = processMetricsRef.current.filter(
      (metric) => !/(ended|interrupted)/i.test(metric.status),
    );
    const activeIds = new Set(
      metrics
        .filter((metric) => !/(ended|interrupted)/i.test(metric.status))
        .map((metric) => metric.session_id),
    );
    for (const metric of metrics) {
      if (/(ended|interrupted)/i.test(metric.status)) continue;
      const sample = sessionPerformanceSamples.current.get(metric.session_id) || {
        cpuTotal: 0,
        cpuCount: 0,
        peakCpu: null,
        peakResidentBytes: null,
      };
      if (metric.cpu_percent !== null) {
        sample.cpuTotal += metric.cpu_percent;
        sample.cpuCount += 1;
        sample.peakCpu = Math.max(sample.peakCpu ?? 0, metric.cpu_percent);
      }
      if (metric.resident_bytes !== null) {
        sample.peakResidentBytes = Math.max(
          sample.peakResidentBytes ?? 0,
          metric.resident_bytes,
        );
      }
      sessionPerformanceSamples.current.set(metric.session_id, sample);
    }
    setProcessMetrics((current) => {
      if (current.length === metrics.length && current.every((metric, index) => {
        const next = metrics[index];
        return metric.session_id === next.session_id &&
          metric.game_id === next.game_id &&
          metric.runtime_seconds === next.runtime_seconds &&
          metric.cpu_percent === next.cpu_percent &&
          metric.resident_bytes === next.resident_bytes &&
          metric.status === next.status;
      })) return current;
      return metrics;
    });
    processMetricsRef.current = metrics;
    setProcessMetricsReady(true);
    const completed = wasActive
      .filter((metric) => !activeIds.has(metric.session_id))
      .at(-1);
    if (completed) {
      setLauncherSessionId(completed.session_id);
      setLauncherDismissed(false);
      void invoke<Session[]>("recent_sessions")
        .then(setSessions)
        .catch(() => {});
    }
  }, []);
  const refreshSessionData = useCallback(() => {
    refreshData().catch(() => {});
  }, [refreshData]);
  const reportProcessMetricsError = useCallback((error: unknown) => {
    setMessage(`Process telemetry unavailable: ${String(error)}`);
  }, []);
  useProcessMetrics(
    call,
    handleProcessMetrics,
    refreshSessionData,
    reportProcessMetricsError,
  );
  const gameRunning = processMetrics.some(
    (metric) => !/(ended|interrupted)/i.test(metric.status),
  );
  gamepadGameRunning.current = gameRunning;
  useEffect(() => {
    let disposed = false;
    let timer: number | undefined;
    const check = async () => {
      if (repoUpdateCheckInFlight.current) {
        pendingRepoUpdateCheck.current = () => void check();
        return;
      }
      repoUpdateCheckInFlight.current = true;
      try {
        const tracked = (await invoke<Repo[]>("list_repos")).filter(
          (repo) => repo.source_path,
        );
        for (const repo of tracked) {
          if (disposed) return;
          try {
            const status = await invoke<string>("check_updates", {
              id: repo.id,
            });
            if (disposed || !status.startsWith("Update available:")) continue;
            const label = `${repo.owner}/${repo.name}`;
            const notificationKey = `${repo.id}:${status}`;
            if (!notifiedUpdates.current.has(notificationKey)) {
              notifiedUpdates.current.add(notificationKey);
              void invoke("desktop_notification", {
                title: "KytyPS5 emulator update available",
                body: `${label}: ${status}`,
              }).catch(() => {});
            }
            if (autoUpdateMode !== "build") {
              setMessage(`${label}: ${status}`);
              continue;
            }
            if (!["official", "kyty-cmake"].includes(repo.adapter)) {
              setMessage(
                `${label}: ${status}. Automatic build is unavailable until the supported Kyty CMake adapter is verified.`,
              );
              continue;
            }
            const attemptKey = `${repo.id}:${status}`;
            if (attemptedAutomaticUpdates.current.has(attemptKey)) {
              setMessage(
                `${label}: automatic build was already attempted this session; review Build Studio logs.`,
              );
              continue;
            }
            if (!processMetricsReady || gameRunning) {
              setMessage(
                `${label}: ${status}. Automatic build is waiting for emulator sessions to finish.`,
              );
              continue;
            }
            attemptedAutomaticUpdates.current.add(attemptKey);
            beginJob();
            pendingJobLogs.current = [];
            setLogs([]);
            setBusy(true);
            setMessage(
              `${label}: update found; building a verified installation in the background.`,
            );
            try {
              const jobId = await invoke<number>("start_update_build", {
                id: repo.id,
                activateAfterBuild: false,
                options: {
                  build_type: "Release",
                  jobs: Math.min(
                    128,
                    Math.max(1, (navigator.hardwareConcurrency || 4) - 1),
                  ),
                  clean: false,
                },
              });
              activateJob(jobId);
              return;
            } catch (error) {
              jobEventGate.current.reset();
              setBusy(false);
              setCurrentBuildPhase("Failed");
              if (jobStartedAt.current !== null)
                setElapsedSeconds(
                  Math.floor((Date.now() - jobStartedAt.current) / 1000),
                );
              jobStartedAt.current = null;
              setMessage(
                `${label}: automatic update build could not start: ${String(error)}`,
              );
            }
          } catch (error) {
            const message = String(error);
            if (
              !disposed &&
              !/already running|preparing an application update/i.test(message)
            )
              setMessage(
                `${repo.owner}/${repo.name}: update check failed: ${message}`,
              );
          }
        }
      } catch (error) {
        if (!disposed) setMessage(`Update checks failed: ${String(error)}`);
      } finally {
        repoUpdateCheckInFlight.current = false;
        const pending = pendingRepoUpdateCheck.current;
        pendingRepoUpdateCheck.current = null;
        pending?.();
      }
    };
    check();
    const hours = Number(updateInterval);
    if (hours > 0) timer = window.setInterval(check, hours * 3600000);
    return () => {
      disposed = true;
      if (timer) window.clearInterval(timer);
    };
  }, [updateInterval, autoUpdateMode, processMetricsReady, gameRunning]);
  useEffect(() => {
    const onFocus = () => {
      gamepadWindowFocused.current = true;
    };
    const onBlur = () => {
      gamepadWindowFocused.current = false;
    };
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  useEffect(() => {
    let previous = new Map<number, boolean>();
    let lastMapping = controllerMappingRef.current;
    const pages: Array<(typeof nav)[number]["id"] | "game-details"> = [
      ...nav.map((item) => item.id),
      "game-details",
    ];
    const timer = window.setInterval(() => {
      const pads = Array.from(navigator.getGamepads?.() || []).filter(
        (pad): pad is Gamepad => !!pad && pad.connected,
      );
      const mapping = controllerMappingRef.current;
      const mappingChanged = mapping !== lastMapping;
      lastMapping = mapping;
      const edge = (pad: Gamepad, key: number, isDown: boolean) => {
        const id = pad.index * 100 + key;
        const was = previous.get(id) || false;
        previous.set(id, isDown);
        return isDown && !was;
      };
      const allowNavigation =
        gamepadWindowFocused.current &&
        !gamepadGameRunning.current &&
        !controllerRemapOpenRef.current &&
        !mappingChanged;
      for (const pad of pads) {
        const directions = [
          mappedControlPressed(pad, mapping.up),
          mappedControlPressed(pad, mapping.down),
          mappedControlPressed(pad, mapping.left),
          mappedControlPressed(pad, mapping.right),
        ];
        const directionEdges = directions.map((down, index) =>
          edge(pad, index, down),
        );
        const direction = directionEdges.findIndex(Boolean);
        if (allowNavigation && direction >= 0) {
          const active = document.activeElement;
          if (
            active instanceof HTMLElement &&
            active.matches(".home-game-row .home-game-tile") &&
            (direction === 2 || direction === 3)
          ) {
            const name = ["up", "down", "left", "right"][direction];
            window.dispatchEvent(
              new CustomEvent("studio:gamepad-move", {
                detail: {
                  direction: name,
                  gameId: Number(active.dataset.gameId),
                  surface: "home",
                },
              }),
            );
          } else if (
            active instanceof HTMLElement &&
            active.matches(".game-grid .game-card")
          ) {
            const name = ["up", "down", "left", "right"][direction];
            window.dispatchEvent(
              new CustomEvent("studio:gamepad-move", {
                detail: {
                  direction: name,
                  gameId: Number(active.dataset.gameId),
                  surface: "library",
                },
              }),
            );
          } else {
            const targets = Array.from(
              document.querySelectorAll<HTMLElement>(
                'button:not(:disabled),input:not(:disabled),select:not(:disabled),[tabindex]:not([tabindex="-1"])',
              ),
            ).filter(
              (element) =>
                element.getClientRects().length > 0 &&
                !element.hasAttribute("hidden"),
            );
            if (targets.length) {
              let current = targets.indexOf(active as HTMLElement);
              if (current < 0)
                current =
                  direction === 0 || direction === 2 ? targets.length : -1;
              const step = direction === 0 || direction === 2 ? -1 : 1;
              targets[
                (current + step + targets.length) % targets.length
              ].focus();
            }
          }
        }
        const confirmPressed = edge(
          pad,
          20,
          mappedControlPressed(pad, mapping.confirm),
        );
        if (allowNavigation && confirmPressed) {
          const active = document.activeElement;
          if (active instanceof HTMLElement && active.matches(".home-game-row .home-game-tile")) {
            active.dispatchEvent(
              new CustomEvent("studio:gamepad-activate", {
                bubbles: true,
                detail: { gameId: Number(active.dataset.gameId) },
              }),
            );
          } else if (active instanceof HTMLElement) active.click();
        }
        const cancelPressed = edge(
          pad,
          21,
          mappedControlPressed(pad, mapping.cancel),
        );
        if (allowNavigation && cancelPressed)
          document.dispatchEvent(
            new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
          );
        const previousPagePressed = edge(
          pad,
          22,
          mappedControlPressed(pad, mapping.previousPage),
        );
        const nextPagePressed = edge(
          pad,
          23,
          mappedControlPressed(pad, mapping.nextPage),
        );
        if (allowNavigation && (previousPagePressed || nextPagePressed)) {
          const delta = previousPagePressed ? -1 : 1;
          setPage(
            (current) =>
              pages[
                (pages.indexOf(current) + delta + pages.length) % pages.length
              ] as (typeof nav)[number]["id"] | "game-details",
          );
        }
      }
    }, 100);
    return () => window.clearInterval(timer);
  }, []);
  const beginJob = () => {
    jobEventGate.current.begin();
    jobStartedAt.current = Date.now();
    setElapsedSeconds(0);
    setCurrentBuildPhase("Starting");
  };
  const action = async (fn: () => Promise<unknown>) => {
    setMessage("");
    try {
      await fn();
      await refreshData();
    } catch (error) {
      setMessage(`Action failed: ${String(error)}`);
    }
  };
  const copyToClipboard = async (text: string, label = "Log") => {
    try {
      await navigator.clipboard.writeText(text);
      setMessage(`${label} copied to clipboard`);
    } catch (error) {
      setMessage(`Could not copy ${label.toLowerCase()}: ${String(error)}`);
    }
  };
  const refreshStorageReport = async () => {
    try {
      setStorageReport(await invoke<StorageReport>("storage_report"));
    } catch (error) {
      setMessage(`Storage refresh failed: ${String(error)}`);
    }
  };
  const cleanStorage = async (
    command: string,
    success: (bytes: number) => string,
    refreshSessions = false,
  ) => {
    try {
      const removed = await invoke<number>(command);
      setMessage(success(removed));
      setStorageReport(await invoke<StorageReport>("storage_report"));
      if (refreshSessions) setSessions(await call<Session[]>("recent_sessions"));
    } catch (error) {
      setMessage(`Storage cleanup failed: ${String(error)}`);
    }
  };
  const checkAppUpdate = async () => {
    if (appUpdateInFlight.current) return;
    appUpdateInFlight.current = true;
    setAppUpdateBusy(true);
    setAppUpdateStatus("Checking GitHub for a newer Studio release…");
    try {
      const info = await invoke<AppUpdateInfo>("check_app_update");
      setAppUpdateInfo(info);
      if (!info.update_available) {
        setAppUpdateStatus(`KytyPS5 Studio ${info.current_version} is up to date.`);
      } else if (!info.asset_name) {
        setAppUpdateStatus(`Version ${info.latest_version} is available, but no installer was published for this platform and architecture.`);
      } else if (info.asset_name.toLowerCase().endsWith(".exe")) {
        setAppUpdateStatus(`Version ${info.latest_version} is available. Choose Install Verified Update to download and verify the NSIS installer.`);
      } else if (!info.running_appimage) {
        setAppUpdateStatus(`Version ${info.latest_version} is available. In-place updates require launching Studio from its AppImage.`);
      } else {
        setAppUpdateStatus(`Version ${info.latest_version} is available. Choose Install Verified AppImage Update to download and verify it.`);
      }
    } catch (error) {
      setAppUpdateStatus(`App update failed: ${String(error)}`);
    } finally {
      appUpdateInFlight.current = false;
      setAppUpdateBusy(false);
    }
  };
  const installAppUpdate = async () => {
    if (!appUpdateInfo?.update_available || !appUpdateInfo.asset_name) return;
    const isWindowsInstaller = appUpdateInfo.asset_name.toLowerCase().endsWith(".exe");
    if (isWindowsInstaller && (gameRunning || busy)) {
      setAppUpdateStatus("Finish active builds and close emulator sessions before installing a Studio update.");
      return;
    }
    if (appUpdateInFlight.current) return;
    appUpdateInFlight.current = true;
    setAppUpdateBusy(true);
    setAppUpdateStatus(`Downloading and verifying the ${isWindowsInstaller ? "NSIS installer" : "AppImage"}…`);
    let installerStarted = false;
    try {
      const result = await invoke<string>("install_app_update");
      if (!isWindowsInstaller) {
        setAppUpdateInfo({ ...appUpdateInfo, current_version: appUpdateInfo.latest_version, update_available: false });
      }
      setAppUpdateStatus(result);
      installerStarted = isWindowsInstaller;
    } catch (error) {
      setAppUpdateStatus(`App update failed: ${String(error)}`);
    } finally {
      if (!installerStarted) {
        appUpdateInFlight.current = false;
        setAppUpdateBusy(false);
      }
    }
  };
  useEffect(() => {
    void checkAppUpdate();
  }, []);
  const launchGame = async (gameId: number) => {
    if (appUpdateBusy) {
      setMessage("Wait for the Studio update to finish before launching a game.");
      return;
    }
    const previousId = sessions.reduce(
      (latest, session) => Math.max(latest, session.id),
      0,
    );
    setMessage("");
    setLauncherDismissed(false);
    try {
      await call("launch_game", { gameId });
      refreshProcessMetricsNow();
    } catch (error) {
      setMessage(String(error));
    }
    try {
      const recent = await call<Session[]>("recent_sessions");
      setSessions(recent);
      const launched = recent.find(
        (session) => session.game_id === gameId && session.id > previousId,
      );
      if (launched) setLauncherSessionId(launched.id);
      await refreshData();
    } catch (error) {
      setMessage(`Could not refresh launcher session: ${String(error)}`);
    }
  };
  const addGameToSteam = async (game: Game) => {
    const preferredBuild = game.build_id ?? builds.find((build) => build.active)?.id ?? builds[0]?.id ?? null;
    setSteamBuildId(preferredBuild);
    setSteamGame(game);
  };
  const confirmAddGameToSteam = async () => {
    if (!steamGame || steamBuildId === null) return;
    setSteamBusy(true);
    setMessage(`Adding ${steamGame.title} to Steam…`);
    try {
      const result = await call<string>("add_game_to_steam", {
        gameId: steamGame.id,
        buildId: steamBuildId,
      });
      setMessage(result);
    } catch (error) {
      setMessage(`Could not add ${steamGame.title} to Steam: ${String(error)}`);
    } finally {
      setSteamBusy(false);
      setSteamGame(null);
    }
  };
  const importDroppedPaths = async (paths: string[]) => {
    let imported = 0;
    let alreadyRegistered = 0;
    let rejected = 0;
    for (const path of paths) {
      try {
        const result = await call<GameScanResult>("scan_games", { path });
        imported += result.imported;
        alreadyRegistered += result.already_registered;
        rejected += result.rejected;
      } catch {
        try {
          await call<Game>("add_game", { path });
          imported++;
        } catch {
          rejected++;
        }
      }
    }
    await refreshData();
    setMessage(
      `${imported} game${imported === 1 ? "" : "s"} imported${alreadyRegistered ? `; ${alreadyRegistered} already registered` : ""}${rejected ? `; ${rejected} path${rejected === 1 ? " was" : "s were"} not recognized` : ""}.`,
    );
  };
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter") {
          setDragActive(true);
        } else if (event.payload.type === "leave") {
          setDragActive(false);
        } else if (event.payload.type === "drop") {
          setDragActive(false);
          void importDroppedPaths(event.payload.paths);
        }
      })
      .then((stop) => {
        unlisten = stop;
      })
      .catch((error) =>
        setMessage(`File drag and drop unavailable: ${String(error)}`),
      );
    return () => unlisten?.();
  }, []);
  const runJob = async (cmd: string, args: Record<string, unknown>) => {
    if (appUpdateBusy || busy || jobStartInFlight.current) {
      setMessage("A task is already running. Wait for it to finish before starting another one.");
      return;
    }
    jobStartInFlight.current = true;
    beginJob();
    pendingJobLogs.current = [];
    setLogs([]);
    setBusy(true);
    try {
      const jobId = await call<number>(cmd, args);
      activateJob(jobId);
    } catch (error) {
      jobEventGate.current.reset();
      setBusy(false);
      setCurrentBuildPhase("Failed");
      setMessage(`Could not start ${cmd}: ${String(error)}`);
      if (jobStartedAt.current !== null)
        setElapsedSeconds(
          Math.floor((Date.now() - jobStartedAt.current) / 1000),
        );
      jobStartedAt.current = null;
    } finally {
      jobStartInFlight.current = false;
    }
  };
  const saveControllerMapping = (mapping: ControllerMapping) => {
    setControllerMapping(mapping);
    try {
      localStorage.setItem("studio.ui.controller_mapping", JSON.stringify(mapping));
    } catch {
      setMessage("Controller mapping could not be saved in this app profile");
    }
    setControllerRemapOpen(false);
  };
  const active = builds.find((b) => b.active);
  const currentGame = games.find((g) => g.id === selectedGame) || games[0];
  const publicGames = useMemo(
    () => games.filter(isPubliclyRecognizedGame),
    [games],
  );
  const libraryCurrentGame = publicGames.find((g) => g.id === selectedGame) || publicGames[0];
  const {
    profile,
    setProfile,
    capabilities: profileCapabilities,
    savedSnapshot: savedProfileSnapshot,
    setSavedSnapshot: setSavedProfileSnapshot,
    loading: profileLoading,
    copyFromGame,
    restore: restoreSavedProfile,
  } = useGameProfile(
    currentGame?.id,
    `${currentGame?.build_id ?? "active"}:${active?.id ?? "none"}`,
    setMessage,
  );
  const featuredGame = useMemo(
    () =>
      publicGames.find((game) => game.id === homeSelection) ||
      ([...publicGames].sort(
        (a, b) => (b.last_played ?? 0) - (a.last_played ?? 0),
      )[0] ??
        null),
    [publicGames, homeSelection],
  );
  const filteredGames = useMemo(
    () => filterAndSortGames(publicGames, query, libraryFilter, librarySort),
    [publicGames, query, libraryFilter, librarySort],
  );
  const vulkanGpus = useMemo(() => {
    const lines = (graphics?.vulkan_details || "").split("\n");
    return lines.flatMap((line, index) => {
      const match = line.trim().match(/^GPU\s*(\d+):?$/);
      if (!match) return [];
      const deviceName = lines
        .slice(index + 1, index + 4)
        .map((value) => value.trim())
        .find((value) => value.startsWith("deviceName ="))
        ?.split("=")
        .slice(1)
        .join("=")
        .trim();
      return deviceName ? [{ index: Number(match[1]), name: deviceName }] : [];
    });
  }, [graphics]);
  const globalUnsupported =
    !!globalProfileCapabilities &&
    Object.keys(globalProfileOptions).some(
      (flag) => !globalProfileCapabilities.options.includes(flag),
    );
  const profileDirty =
    !profileLoading &&
    savedProfileSnapshot !== "" &&
    JSON.stringify(profile) !== savedProfileSnapshot;
  const selectGameWithGuard = (id: number) => {
    if (
      profileDirty &&
      !window.confirm(
        "This game profile has unsaved changes. Discard them and switch games?",
      )
    )
      return false;
    setSelectedGame(id);
    return true;
  };
  const hasUnsupportedProfile =
    !!profileCapabilities &&
    ((profile.screen_width !== null && !profileCapabilities.screen_width) ||
      (profile.screen_height !== null && !profileCapabilities.screen_height) ||
      (profile.present_mode !== null && !profileCapabilities.present_mode) ||
      (profile.perf_overlay && !profileCapabilities.perf_overlay) ||
      (profile.fullscreen && !profileCapabilities.fullscreen) ||
      (profile.controller_color !== null &&
        !profileCapabilities.controller_color) ||
      (profile.controller_volume !== null &&
        !profileCapabilities.controller_volume) ||
      (profile.controller_vibration !== null &&
        !profileCapabilities.controller_vibration) ||
      (profile.keyboard_player !== null &&
        !profileCapabilities.keyboard_player) ||
      Object.keys(profile.options).some(
        (flag) => !profileCapabilities.options.includes(flag),
      ));
  useEffect(() => {
    setGameCompatibility(null);
    if (currentGame)
      call<GameCompatibility>("game_compatibility", { gameId: currentGame.id })
        .then(setGameCompatibility)
        .catch(() => {});
  }, [currentGame?.id]);
  useEffect(() => {
    setOfficialCompatibilityDetails(null);
    setOfficialCompatibilityError(null);
    if (!currentGame?.title_id) return;
    let disposed = false;
    const refreshDetails = () => {
      void invoke<OfficialCompatibilityDetails | null>("official_compatibility_details", { titleId: currentGame.title_id })
        .then((result) => { if (!disposed) { setOfficialCompatibilityDetails(result); setOfficialCompatibilityError(null); } })
        .catch((error) => { if (!disposed) setOfficialCompatibilityError(String(error)); });
    };
    refreshDetails();
    const timer = window.setInterval(refreshDetails, 6 * 60 * 60 * 1000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [currentGame?.id, currentGame?.title_id]);
  useEffect(() => {
    setCompatibilityObservations([]);
    if (currentGame)
      call<ManualCompatibilityObservation[]>("compatibility_observations", {
        gameId: currentGame.id,
      })
        .then(setCompatibilityObservations)
        .catch(() => {});
  }, [currentGame?.id]);
  useEffect(() => {
    if (page !== "diagnostics" && !(page === "home" && specsOpen)) return;
    let disposed = false;
    setGraphics(null);
    invoke<GraphicsDiagnostics>("inspect_graphics")
      .then((result) => {
        if (!disposed) setGraphics(result);
      })
      .catch((error) => {
        if (!disposed)
          setMessage(`Graphics diagnostics unavailable: ${String(error)}`);
      });
    return () => {
      disposed = true;
    };
  }, [page, specsOpen]);
  useEffect(() => {
    let disposed = false;
    if (page === "library" && currentGame) {
      const release = retainGameIcon(currentGame.id);
      void loadGameIcon(currentGame.id).then((url) => {
        if (!disposed) setGameIcon(url);
      });
      return () => {
        disposed = true;
        release();
      };
    }
    setGameIcon(null);
    return () => {
      disposed = true;
    };
  }, [currentGame?.id, page]);
  useEffect(() => {
    let disposed = false;
    let timer = 0;
    if (page === "library" && currentGame) {
      const release = retainGameBackground(currentGame.id);
      timer = window.setTimeout(() => {
        void loadGameBackground(currentGame.id).then((url) => {
          if (!disposed) setGameBackground(url);
        });
      }, 250);
      return () => {
        disposed = true;
        window.clearTimeout(timer);
        release();
      };
    }
    setGameBackground(null);
    return () => {
      disposed = true;
    };
  }, [currentGame?.id, page]);
  useEffect(() => {
    let disposed = false;
    if (page === "home" && featuredGame) {
      const release = retainGameBackground(featuredGame.id);
      void loadGameBackground(featuredGame.id).then((url) => {
        if (!disposed) setFeaturedBackground(url);
      });
      return () => {
        disposed = true;
        release();
      };
    }
    setFeaturedBackground(null);
    return () => {
      disposed = true;
    };
  }, [featuredGame?.id, page]);
  const choose = async (kind: "file" | "directory") => {
    const chosen = await open({
      directory: kind === "directory",
      multiple: false,
    });
    if (typeof chosen === "string")
      action(async () => {
        if (kind === "directory") {
          const result = await call<GameScanResult>("scan_games", {
            path: chosen,
          });
          setMessage(
            `${result.imported} new game${result.imported === 1 ? "" : "s"} imported${result.already_registered ? `; ${result.already_registered} already in the library` : ""}${result.rejected ? `; ${result.rejected} unsupported or inaccessible entry${result.rejected === 1 ? "" : "ies"} skipped` : ""}. Install sizes are calculated in the background.`,
          );
        } else {
          await call("add_game", { path: chosen });
          setMessage(
            "Game registered; install size is being calculated in the background",
          );
        }
      });
  };
  const chooseSingleGameFolder = async () => {
    const chosen = await open({ directory: true, multiple: false });
    setImportMenuOpen(false);
    if (typeof chosen !== "string") return;
    action(async () => {
      await call("add_game", { path: chosen });
      setMessage(
        "Game registered; install size is being calculated in the background",
      );
    });
  };
  useEffect(() => {
    if (!importMenuOpen) return;
    const closeOnOutsideClick = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !importMenuRef.current?.contains(event.target)
      ) {
        setImportMenuOpen(false);
      }
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setImportMenuOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsideClick);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsideClick);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [importMenuOpen]);
  const maximizeWindow = () => {
    void getCurrentWindow()
      .toggleMaximize()
      .catch((error) => setMessage(`Window control failed: ${String(error)}`));
  };
  const dragWindow = (event: React.MouseEvent) => {
    const target = event.target as HTMLElement;
    if (event.button === 0 && !target.closest("button,a,input,select"))
      void getCurrentWindow()
        .startDragging()
        .catch(() => {});
  };
  const activeSession = processMetrics.find(
    (metric) => !/(ended|interrupted)/i.test(metric.status),
  );
  const activeSessionGame = activeSession
    ? games.find((game) => game.id === activeSession.game_id)
    : undefined;
  const activeSessionRecord = activeSession
    ? sessions.find((session) => session.id === activeSession.session_id)
    : undefined;
  useEffect(() => {
    if (activeSession) {
      setLauncherSessionId(activeSession.session_id);
      setLauncherDismissed(false);
      setHomeSelection(activeSession.game_id);
      invoke<Session[]>("recent_sessions")
        .then(setSessions)
        .catch(() => {});
    }
  }, [activeSession?.session_id]);
  const launcherSession =
    sessions.find((session) => session.id === launcherSessionId) ||
    activeSessionRecord;
  const launcherMetric = launcherSession
    ? processMetrics.find((metric) => metric.session_id === launcherSession.id)
    : activeSession;
  const launcherGame = launcherSession
    ? games.find((game) => game.id === launcherSession.game_id)
    : activeSessionGame;
  const launcherLive =
    !!launcherMetric && !/(ended|interrupted)/i.test(launcherMetric.status);
  const launcherStatus = launcherLive
    ? launcherMetric.status
    : launcherSession?.exit_code !== null &&
        launcherSession?.exit_code !== undefined
      ? `Exited · code ${launcherSession.exit_code}`
      : launcherSession?.outcome || "Session ended";
  const launcherRuntime = launcherLive
    ? launcherMetric.runtime_seconds
    : launcherSession
      ? Math.max(
          0,
          (launcherSession.ended_at ?? Math.floor(Date.now() / 1000)) -
            launcherSession.started_at,
        )
      : 0;
  const launcherSamples = launcherSession
    ? sessionPerformanceSamples.current.get(launcherSession.id)
    : undefined;
  const launcherPerformance = {
    averageCpuPercent:
      launcherSamples && launcherSamples.cpuCount > 0
        ? launcherSamples.cpuTotal / launcherSamples.cpuCount
        : null,
    peakCpuPercent: launcherSamples?.peakCpu ?? null,
    peakResidentBytes: launcherSamples?.peakResidentBytes ?? null,
  };
  const [launcherIcon, setLauncherIcon] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    if (launcherGame) {
      const release = retainGameIcon(launcherGame.id);
      void loadGameIcon(launcherGame.id).then((url) => {
        if (!disposed) setLauncherIcon(url);
      });
      return () => {
        disposed = true;
        release();
      };
    }
    setLauncherIcon(null);
    return () => {
      disposed = true;
    };
  }, [launcherGame?.id]);
  const [launcherLog, setLauncherLog] = useState("");
  useEffect(() => {
    const path = launcherSession?.log_path;
    if (!path) {
      setLauncherLog("");
      return;
    }
    let disposed = false;
    const read = () =>
      invoke<string>("read_log", { path })
        .then((text) => {
          if (!disposed) setLauncherLog(text);
        })
        .catch((error) => {
          if (!disposed) setLauncherLog(`Log unavailable: ${String(error)}`);
        });
    read();
    const timer = window.setInterval(read, 5000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [launcherSession?.log_path]);
  useEffect(() => {
    let alive = true;
    invoke<Record<string, string>>("get_global_profile_options")
      .then((options) => {
        if (alive) setGlobalProfileOptions(options);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);
  useEffect(() => {
    if (page !== "settings") return;
    let alive = true;
    Promise.all([
      call<Record<string, string>>("get_global_profile_options"),
      call<ProfileCapabilities>("get_active_profile_capabilities"),
    ])
      .then(([options, caps]) => {
        if (alive) {
          setGlobalProfileOptions(options);
          setGlobalProfileCapabilities(caps);
        }
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [page, active?.id]);
  const copyProfileFromGame = async (sourceGameId: number) => {
    if (
      profileDirty &&
      !window.confirm(
        "Discard the current unsaved profile changes and copy another game profile?",
      )
    )
      return;
    const copied = await copyFromGame(sourceGameId);
    if (copied) {
      setMessage(
        "Profile copied into the editor. Save the profile to apply it to this game.",
      );
    }
  };
  const gameProfileEditor = currentGame ? (
    <GameProfileEditor
      gameId={currentGame.id}
      games={games}
      profile={profile}
      setProfile={setProfile}
      capabilities={profileCapabilities}
      globalOptions={globalProfileOptions}
      gpuDevices={vulkanGpus}
      sourceGameId={profileCopySource}
      setSourceGameId={setProfileCopySource}
      loading={profileLoading}
      dirty={profileDirty}
      canRestore={!!savedProfileSnapshot}
      unsupported={hasUnsupportedProfile}
      onCopy={(gameId) => void copyProfileFromGame(gameId)}
      onRestore={restoreSavedProfile}
      onClearUnsupported={() =>
        setProfile((current) =>
          profileWithOptions(
            current,
            Object.fromEntries(
              Object.entries(current.options).filter(([flag]) =>
                profileCapabilities?.options.includes(flag),
              ),
            ),
          ),
        )
      }
      onSave={() =>
        action(async () => {
          await call("set_profile", { gameId: currentGame.id, profile });
          setSavedProfileSnapshot(JSON.stringify(profile));
          setMessage("Game profile saved");
        })
      }
      onReset={() =>
        setProfile({
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
        })
      }
    />
  ) : null;
  return (
    <div className="app">
      {dragActive && (
        <div className="drop-overlay">
          Drop supported game folders, ELF files, or ZArchives to import
        </div>
      )}
      <header
        className="topbar app-titlebar"
        onMouseDown={dragWindow}
        onDoubleClick={(event) => {
          if (event.target === event.currentTarget) maximizeWindow();
        }}
      >
        <button
          className="brand app-brand"
          onClick={() => setPage("home")}
          onDoubleClick={maximizeWindow}
          aria-label="KytyPS5 Studio home"
        >
          <span className="wordmark">KytyPS5 Studio</span>
        </button>
        <MainNavigation
          items={nav.filter(
            (item) => item.id !== "library" && item.id !== "settings",
          )}
          active={page}
          onNavigate={(id) => setPage(id as (typeof nav)[number]["id"])}
        />
        <div className="title-tools">
          <div className="import-menu-wrap" ref={importMenuRef}>
            <button
              className="title-action"
              aria-haspopup="menu"
              aria-expanded={importMenuOpen}
              onClick={() => setImportMenuOpen((open) => !open)}
            >
              <span>＋</span> Import
            </button>
            {importMenuOpen && (
              <div className="import-menu" role="menu" aria-label="Import games">
                <button
                  role="menuitem"
                  onClick={() => {
                    setImportMenuOpen(false);
                    void choose("directory");
                  }}
                >
                  <strong>Scan folder location</strong>
                  <small>Find and import all supported games inside</small>
                </button>
                <button role="menuitem" onClick={() => void chooseSingleGameFolder()}>
                  <strong>Add one game folder</strong>
                  <small>Select a specific game folder to add</small>
                </button>
              </div>
            )}
          </div>
          <span
            className="title-game-count"
            aria-label={`${games.length} games in library`}
            title="Games in library"
          >
            Games ({games.length})
          </span>
          <button
            onClick={() => refreshSystem()}
            className="title-icon refresh-button"
            aria-label="Refresh library and system status"
            title="Refresh"
          >
            <RefreshCw size={14} />
          </button>
          <button
            onClick={() => setPage("settings")}
            className="title-icon settings-button"
            aria-label="Settings"
          >
            <Settings size={15} />
          </button>
          <span className="clock-label">
            {new Date().toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </span>
          <div className="window-controls">
            <button
              onClick={() =>
                void getCurrentWindow()
                  .minimize()
                  .catch(() => {})
              }
              aria-label="Minimize"
            >
              <Minus size={13} />
            </button>
            <button onClick={maximizeWindow} aria-label="Maximize">
              <Maximize2 size={12} />
            </button>
            <button
              className="close-window"
              onClick={() =>
                void getCurrentWindow()
                  .close()
                  .catch(() => {})
              }
              aria-label="Close"
            >
              <X size={13} />
            </button>
          </div>
        </div>
      </header>
      <main>
        <div className="main-content">
          {page !== "home" && message && !message.includes("Update available:") && (
            <div className="notice" onClick={() => setMessage("")}>
              <CircleAlert size={17} />
              {message}
              <span>×</span>
            </div>
          )}
          {page === "home" && (
            <HomePage
              games={publicGames}
              officialCompatibilityIndex={officialCompatibilityIndex}
              featuredGame={featuredGame}
              featuredBackground={featuredBackground}
              active={active}
              repos={repos}
              system={system}
              graphics={graphics}
              specsOpen={specsOpen}
              setSpecsOpen={setSpecsOpen}
              message={message}
              setPage={setPage}
              choose={choose}
              launchGame={launchGame}
              onAddToSteam={addGameToSteam}
              selectGameWithGuard={selectGameWithGuard}
              setGameDetailsTab={setGameDetailsTab}
              setHomeSelection={setHomeSelection}
              inspectGameContent={inspectGameContent}
              runningGameId={activeSessionGame?.id}
              onShowRunningSession={() => setLauncherDismissed(false)}
              onRemoveGame={(gameId) =>
                action(async () => {
                  await call("remove_game_entry", { gameId });
                  setSelectedGame((current) => current === gameId ? null : current);
                  setHomeSelection((current) => current === gameId ? null : current);
                  setMessage("Game entry removed; files and session history were kept");
                })
              }
            />
          )}
          {page === "forks" && (
            <RepositoryManagerPage
              forks={forks}
              repos={repos}
              currentRepo={currentRepo}
              selectedRepo={selectedRepo}
              repoUrl={repoUrl}
              setRepoUrl={setRepoUrl}
              githubDetails={githubDetails}
              busy={busy}
              onDiscoverForks={() =>
                action(async () =>
                  setForks(await call<Fork[]>("discover_forks")),
                )
              }
              onAddManualRepository={() =>
                action(async () => {
                  const repo = await call<Repo>("add_repository", {
                    url: repoUrl,
                    branch: "",
                  });
                  setSelectedRepo(repo.id);
                  setRepoUrl("");
                })
              }
              onAddOfficialRepository={() =>
                action(async () => {
                  const repo = await call<Repo>("add_repository", {
                    url: "https://github.com/KytyPS5/KytyPS5",
                    branch: "",
                  });
                  setSelectedRepo(repo.id);
                  setMessage(`${repo.owner}/${repo.name} selected as the official source. Use Build to create an emulator installation.`);
                })
              }
              onSelectRepository={(id) => {
                setSelectedRepo(id);
                setGithubDetails(null);
              }}
              onCloneRepository={(id) => runJob("start_clone", { id })}
              onOpenSourceFolder={(id) =>
                action(() => call("open_repository_folder", { id }))
              }
              onInspectRepository={(id) =>
                action(() => call("inspect_repository", { id }))
              }
              onCheckRepositoryUpdates={(id) =>
                call<string>("check_updates", { id })
              }
              onRefreshGitHubDetails={(id) =>
                action(async () =>
                  setGithubDetails(
                    await call<GithubDetails>("github_repository_details", {
                      id,
                    }),
                  ),
                )
              }
              onOpenBuildStudio={() => setPage("build")}
              onAddFork={(fork) =>
                action(async () => {
                  const repo = await call<Repo>("add_repository", {
                    url: fork.html_url,
                    branch: fork.default_branch,
                  });
                  setSelectedRepo(repo.id);
                })
              }
              onRemoveRepository={(id) =>
                action(async () => {
                  const result = await call<string>("remove_repository", { id });
                  if (selectedRepo === id) setSelectedRepo(null);
                  setGithubDetails(null);
                  setMessage(result);
                })
              }
              onUpdateNow={(id) => {
                setPage("build");
                void runJob("start_update_build", {
                  id,
                  activateAfterBuild: true,
                  options: { build_type: buildType, jobs, clean: cleanBuild },
                });
              }}
            />
          )}
          {page === "community" && (
            <section className="page community-page">
              <div className="sectionhead">
                <div>
                  <h3>Community PR Builds</h3>
                  <p>
                    Select one or more GitHub pull requests and build them
                    into an isolated version of the selected repository.
                  </p>
                </div>
                <button
                  className="outline"
                  disabled={communityLoading || !selectedRepo}
                  onClick={refreshCommunityPRs}
                  title="Refresh pull requests from GitHub"
                >
                  <RefreshCw size={16} /> Refresh PRs
                </button>
              </div>
              <CommunityPRFilters
                repos={trackedRepos}
                selectedRepo={selectedRepo}
                state={communityState}
                search={communitySearch}
                number={communityNumber}
                label={communityLabel}
                mergedOnly={communityMergedOnly}
                sort={communitySort}
                onRepositoryChange={(id) => {
                  setSelectedRepo(id);
                  setCommunityPage(1);
                  setSelectedPrs([]);
                  setSelectionReview([]);
                  setCommunityPinnedHeads({});
                }}
                onStateChange={(state) => {
                  setCommunityState(state);
                  setCommunityPage(1);
                }}
                onSearchChange={(search) => {
                  setCommunitySearch(search);
                  setCommunityPage(1);
                }}
                onNumberChange={(number) => {
                  setCommunityNumber(number);
                  setCommunityPage(1);
                }}
                onLabelChange={(label) => {
                  setCommunityLabel(label);
                  setCommunityPage(1);
                }}
                onMergedOnlyChange={(enabled) => {
                  setCommunityMergedOnly(enabled);
                  if (enabled && communityState === "open")
                    setCommunityState("closed");
                }}
                onSortChange={setCommunitySort}
              />
              <div className="community-stats">
                <span>
                  <strong>{communityPrs?.total_count ?? "—"}</strong>{" "}
                  {communityState} pull requests
                </span>
                <span>
                  <strong>
                    {communityHistory.some((entry) => entry.active)
                      ? "Active experimental build"
                      : active
                        ? "Active official/fork build"
                        : "No active build"}
                  </strong>
                </span>
                <span>
                  <strong>
                    Base {short(communityPrs?.items[0]?.base_sha ?? null)}
                  </strong>
                </span>
                <span>
                  <strong>{selectedPrs.length}</strong> selected for build
                </span>
                <span>
                  <strong>
                    {communityPrs
                      ? communityHistory.filter(
                          (entry) =>
                            entry.repository === communityPrs?.repository &&
                            entry.pull_requests.some(([number, sha]) =>
                              communityPrs.items.some(
                                (pr) =>
                                  pr.number === number && pr.head_sha !== sha,
                              ),
                            ),
                        ).length
                      : 0}
                  </strong>{" "}
                  available updates in loaded PR page
                </span>
                <span>
                  {communityPrs?.stale
                    ? "Cached results · refresh failed"
                    : "Build compatibility is unverified until an actual build succeeds"}
                </span>
              </div>
              {communityPrs?.refresh_error && (
                <p className="diagnosis-note">
                  GitHub refresh issue: {communityPrs.refresh_error}
                </p>
              )}
              {communityPrs?.truncated && (
                <p className="diagnosis-note">
                  GitHub search shows at most its first 1,000 matches. Refine
                  the search to reach older results.
                </p>
              )}
              {!selectedRepo ? (
                <div className="empty">
                  <GitBranch />
                  <strong>Add and clone a repository first</strong>
                  <p>
                    PR discovery is scoped to one tracked KytyPS5 repository.
                  </p>
                </div>
              ) : (
                <>
                  <CommunityPRTable
                    pullRequests={communityVisible}
                    selectedPullRequests={selectedPrs}
                    selectedDetails={communityDetails}
                    loading={communityLoading}
                    onSelectPage={(selected) => {
                      setSelectionReview([]);
                      setCommunityPinnedHeads({});
                      setSelectedPrs((current) => {
                        const pageNumbers = new Set(
                          communityVisible.map((pr) => pr.number),
                        );
                        const retained = current.filter(
                          (pr) => !pageNumbers.has(pr.number),
                        );
                        return selected
                          ? [...retained, ...communityVisible]
                          : retained;
                      });
                    }}
                    onSelectionChange={(pr, selected) => {
                      setSelectionReview([]);
                      setCommunityPinnedHeads({});
                      setSelectedPrs((current) =>
                        selected
                          ? [...current, pr]
                          : current.filter((item) => item.number !== pr.number),
                      );
                    }}
                    onOpenDetails={(pr) => {
                      if (!selectedRepo) return;
                      invoke<CommunityPRDetails>(
                        "community_pull_request_details",
                        {
                          repositoryId: selectedRepo,
                          number: pr.number,
                        },
                      )
                        .then(setCommunityDetails)
                        .catch((error) => setMessage(String(error)));
                    }}
                  />
                  <div className="community-pagination">
                    <button
                      className="outline"
                      disabled={communityPage <= 1 || communityLoading}
                      onClick={() => setCommunityPage((value) => value - 1)}
                    >
                      Previous
                    </button>
                    <span>
                      Page {communityPage}
                      {communityPrs?.total_count !== null &&
                      communityPrs?.total_count !== undefined
                        ? ` · ${communityPrs.total_count} total`
                        : ""}
                    </span>
                    <button
                      className="outline"
                      disabled={!communityPrs?.has_more || communityLoading}
                      onClick={() => setCommunityPage((value) => value + 1)}
                    >
                      Next
                    </button>
                  </div>
                </>
              )}
              <CommunityPRSelection
                selectedPullRequests={selectedPrs}
                review={selectionReview}
                targetRepository={
                  communityPrs?.repository || "Selected repository"
                }
                targetBranch={
                  selectedPrs[0]?.base_branch ||
                  communityPrs?.items[0]?.base_branch ||
                  "default branch"
                }
                pinnedHeads={communityPinnedHeads}
                busy={busy}
                reviewLoading={reviewLoading}
                buildType={buildType}
                jobs={jobs}
                cleanBuild={cleanBuild}
                onMove={(index, direction) => {
                  setSelectionReview([]);
                  setSelectedPrs((items) => {
                    const next = [...items];
                    const target = index + direction;
                    if (target < 0 || target >= next.length) return items;
                    [next[index], next[target]] = [next[target], next[index]];
                    return next;
                  });
                }}
                onClear={() => {
                  setSelectedPrs([]);
                  setSelectionReview([]);
                  setCommunityPinnedHeads({});
                }}
                onReview={() => void reviewCommunitySelection()}
                onBuild={(review) => {
                  const reviewed = review
                    .map(
                      (detail) =>
                        `#${detail.pull_request.number} @ ${detail.pull_request.head_sha.slice(0, 12)}`,
                    )
                    .join(" + ");
                  const targetBranch =
                    review[0]?.pull_request.base_branch ?? "target branch";
                  const approved = window.confirm(
                    `Build exactly ${reviewed} in ${communityPrs?.repository ?? "the selected repository"} on ${targetBranch}. Studio applies the PRs in the selected order to the latest branch commit. This runs community code. Confirm only after reviewing all listed files.`,
                  );
                  if (approved && selectedRepo)
                    void runJob("start_community_build", {
                      repositoryId: selectedRepo,
                      pullRequests: review.map((detail) => ({
                        number: detail.pull_request.number,
                        head_sha: detail.pull_request.head_sha,
                        base_sha: detail.pull_request.base_sha,
                      })),
                      options: {
                        build_type: buildType,
                        jobs,
                        clean: cleanBuild,
                      },
                    });
                }}
                onBuildTypeChange={setBuildType}
                onJobsChange={setJobs}
                onCleanChange={setCleanBuild}
              />
              <CommunityBuildHistory
                history={communityHistory}
                pullRequests={communityPrs}
                busy={busy}
                reviewLoading={reviewLoading}
                onActivate={(id) =>
                  action(() => call("activate_build", { id }))
                }
                onReview={(entry) => void reviewHistoryHeads(entry)}
                onRename={(entry) => {
                  const name = window.prompt(
                    "Rename community build configuration",
                    entry.name,
                  );
                  if (name === null) return;
                  action(async () => {
                    await call("rename_community_build_config", {
                      configId: entry.config_id,
                      name,
                    });
                    setCommunityHistory(
                      await call<CommunityBuildHistoryEntry[]>(
                        "community_build_history",
                      ),
                    );
                  });
                }}
              />
              <CommunityPullRequestDrawer
                details={communityDetails}
                onClose={() => setCommunityDetails(null)}
              />
            </section>
          )}
          {page === "build" && (
            <BuildStudioPage
              repos={trackedRepos}
              currentRepo={currentRepo}
              buildSourceInfo={buildSourceInfo}
              system={system}
              buildMode={buildMode}
              setBuildMode={setBuildMode}
              buildType={buildType}
              setBuildType={setBuildType}
              jobs={jobs}
              setJobs={setJobs}
              cleanBuild={cleanBuild}
              setCleanBuild={setCleanBuild}
              activateAfterBuild={activateAfterBuild}
              setActivateAfterBuild={setActivateAfterBuild}
              busy={busy}
              currentBuildPhase={currentBuildPhase}
              elapsedSeconds={elapsedSeconds}
              logs={logs}
              onSelectRepo={setSelectedRepo}
              onAddOfficialRepository={() =>
                action(async () => {
                  const repo = await call<Repo>("add_repository", {
                    url: "https://github.com/KytyPS5/KytyPS5",
                    branch: "",
                  });
                  setSelectedRepo(repo.id);
                  setMessage(`${repo.owner}/${repo.name} selected. Clone its source to continue.`);
                })
              }
              onCloneRepository={(id) => runJob("start_clone", { id })}
              onBuild={() =>
                runJob("start_build", {
                  id: currentRepo!.id,
                  activateAfterBuild,
                  options: { build_type: buildType, jobs, clean: cleanBuild },
                })
              }
              onBuildLatest={() =>
                runJob("start_update_build", {
                  id: currentRepo!.id,
                  activateAfterBuild,
                  options: { build_type: buildType, jobs, clean: cleanBuild },
                })
              }
              onInstallTools={() => runJob("start_install_build_tools", {})}
              onCancel={() => action(() => call("cancel_job"))}
              onCopyLog={() => void copyToClipboard(logs.join("\n"), "Build log")}
            />
          )}
          {page === "emulators" && (
            <EmulatorManagerPage
              builds={builds}
              repos={repos}
              games={games}
              sessions={sessions}
              selectedLog={selectedLog}
              busy={busy}
              onNewBuild={() => setPage("build")}
              onOpenForkManager={() => setPage("forks")}
              onOpenBuildFolder={(id) =>
                action(() => call("open_build_folder", { id }))
              }
              onVerifyBuild={(id) => {
                void call<string>("validate_build", { id })
                  .then(setMessage)
                  .catch(() => {})
                  .finally(() => refreshData().catch(() => {}));
              }}
              onCheckUpdates={(repoId) =>
                action(() =>
                  call<string>("check_updates", { id: repoId }).then(
                    setMessage,
                  ),
                )
              }
              onRebuild={(build) =>
                runJob("start_build", {
                  id: build.repo_id,
                  options: { build_type: build.build_type, jobs, clean: false },
                })
              }
              onActivateBuild={(id) =>
                action(() => call("activate_build", { id }))
              }
              onOpenBuildLog={(id) =>
                action(async () => {
                  setSelectedLogPath(null);
                  setSelectedLog(await call<string>("read_build_log", { id }));
                })
              }
              onRemoveBuild={(id) =>
                action(async () => {
                  await call("remove_build", { id });
                  await refreshData();
                  setMessage("Managed emulator build removed");
                })
              }
              onCopyLog={() => void copyToClipboard(selectedLog, "Build log")}
            />
          )}
          {page === "library" && (
            <GameLibraryPage
              games={publicGames}
              filteredGames={filteredGames}
              currentGame={libraryCurrentGame}
              gameBackground={gameBackground}
              gameIcon={gameIcon}
              gameScan={gameScan}
              gameCompatibility={gameCompatibility}
              officialCompatibilityIndex={officialCompatibilityIndex}
              inspectGameContent={inspectGameContent}
              query={query}
              setQuery={setQuery}
              libraryFilter={libraryFilter}
              setLibraryFilter={setLibraryFilter}
              librarySort={librarySort}
              setLibrarySort={setLibrarySort}
              libraryView={libraryView}
              setLibraryView={setLibraryView}
              builds={builds}
              repos={repos}
              profileDirty={profileDirty}
              onChoose={choose}
              onSelectGame={selectGameWithGuard}
              onRemoveGame={(gameId) =>
                action(async () => {
                  await call("remove_game_entry", { gameId });
                  setSelectedGame((current) => current === gameId ? null : current);
                  setHomeSelection((current) => current === gameId ? null : current);
                  setMessage("Game entry removed; files and session history were kept");
                })
              }
              onOpenDetails={() => {
                setGameDetailsTab("Overview");
                setPage("game-details");
              }}
              onOpenGameFolder={() =>
                action(() =>
                  call("open_game_folder", { gameId: currentGame!.id }),
                )
              }
              onOpenFolderForGame={(game) =>
                action(() => call("open_game_folder", { gameId: game.id }))
              }
              onOpenLatestSessionLog={() =>
                action(async () => {
                  const path = gameCompatibility!.latest_log_path!;
                  setSelectedLogPath(path);
                  setSelectedLog(await call<string>("read_log", { path }));
                  setPage("diagnostics");
                })
              }
              onLaunch={(id) => void launchGame(id)}
              onAddToSteam={addGameToSteam}
              onToggleFavorite={(id) =>
                action(() => call("toggle_favorite", { gameId: id }))
              }
              onSetPreferredBuild={(buildId) =>
                action(() =>
                  call("set_game_build", { gameId: currentGame!.id, buildId }),
                )
              }
            />
          )}
          {page === "diagnostics" && (
            <DiagnosticsPage
              system={system}
              graphics={graphics}
              processMetrics={processMetrics}
              sessions={sessions}
              selectedLog={selectedLog}
              selectedLogPath={selectedLogPath}
              logFilter={logFilter}
              setLogFilter={setLogFilter}
              onSelectSessionLog={(session) =>
                action(async () => {
                  setSelectedLogPath(session.log_path);
                  setSelectedLog(
                    await call<string>("read_log", { path: session.log_path }),
                  );
                })
              }
              onExportLog={() => action(exportSelectedLog)}
              onCopyLog={() => void copyToClipboard(selectedLog)}
            />
          )}
          {page === "game-details" && currentGame && (
            <GameDetailsPage
              game={currentGame}
              tab={gameDetailsTab}
              setTab={setGameDetailsTab}
              profileDirty={profileDirty}
              builds={builds}
              repos={repos}
              gameProfileEditor={gameProfileEditor}
              gameCompatibility={gameCompatibility}
              officialCompatibilityIndex={officialCompatibilityIndex}
              officialCompatibilityDetails={officialCompatibilityDetails}
              officialCompatibilityError={officialCompatibilityError}
              manualOutcome={manualOutcome}
              setManualOutcome={setManualOutcome}
              manualNotes={manualNotes}
              setManualNotes={setManualNotes}
              compatibilityObservations={compatibilityObservations}
              sessions={sessions}
              onBack={() => setPage("library")}
              onLaunch={(gameId) => void launchGame(gameId)}
              onAddToSteam={addGameToSteam}
              onOpenGameFolder={() =>
                action(() =>
                  call("open_game_folder", { gameId: currentGame.id }),
                )
              }
              onRefreshMetadata={() =>
                action(async () => {
                  await call("refresh_game_metadata", {
                    gameId: currentGame.id,
                  });
                  setMessage("Game metadata refresh started");
                })
              }
              onRemoveEntry={() =>
                action(async () => {
                  await call("remove_game_entry", { gameId: currentGame.id });
                  setPage("library");
                  setSelectedGame(null);
                  await refreshData();
                  setMessage(
                    "Game entry removed; files and session history were kept",
                  );
                })
              }
              onSetPreferredBuild={(buildId) =>
                action(() =>
                  call("set_game_build", { gameId: currentGame.id, buildId }),
                )
              }
              onActivateBuild={(buildId) =>
                action(async () => {
                  await call("activate_build", { id: buildId });
                  await refreshBuilds();
                  setMessage("Recommended emulator build activated");
                })
              }
              onBuildRecommendedSource={(repoId) => {
                setSelectedRepo(repoId);
                setPage("build");
              }}
              onRecordObservation={() =>
                action(async () => {
                  await call("record_compatibility_observation", {
                    gameId: currentGame.id,
                    outcome: manualOutcome,
                    notes: manualNotes,
                  });
                  setCompatibilityObservations(
                    await call<ManualCompatibilityObservation[]>(
                      "compatibility_observations",
                      { gameId: currentGame.id },
                    ),
                  );
                  setManualNotes("");
                  setMessage("Manual compatibility observation saved");
                })
              }
              onOpenSessionLog={(session) =>
                action(async () => {
                  setSelectedLogPath(session.log_path);
                  setSelectedLog(
                    await call<string>("read_log", { path: session.log_path }),
                  );
                  setPage("diagnostics");
                })
              }
              onInspectContent={inspectGameContent}
              onFetchOnlineCheats={fetchGameCheats}
              onDownloadOnlineCheat={downloadGameCheat}
              onImportCustomPatch={importCustomGamePatch}
              onSaveCustomPatch={saveCustomGamePatch}
              onSelectCustomPatch={(path) =>
                setProfile((current) => ({
                  ...current,
                  options: { ...current.options, "--game-patch": path },
                }))
              }
            />
          )}
          {page === "game-details" && !currentGame && (
            <section className="page">
              <div className="empty">
                <BookOpen />
                <strong>Select a game to view its details</strong>
                <button className="outline" onClick={() => setPage("library")}>
                  Open library
                </button>
              </div>
            </section>
          )}
          {page === "settings" && (
            <SettingsPage
              globalProfileCapabilities={globalProfileCapabilities}
              globalProfileOptions={globalProfileOptions}
              setGlobalProfileOptions={setGlobalProfileOptions}
              globalUnsupported={globalUnsupported}
              gpuDevices={vulkanGpus}
              githubTokenAvailable={githubTokenAvailable}
              githubTokenError={githubTokenError}
              githubTokenInput={githubTokenInput}
              setGithubTokenInput={setGithubTokenInput}
              updateInterval={updateInterval}
              discordPresenceEnabled={discordPresenceEnabled}
              autoUpdateMode={autoUpdateMode}
              storageReport={storageReport}
              appUpdate={appUpdateInfo}
              appUpdateStatus={appUpdateStatus}
              appUpdateBusy={appUpdateBusy}
              busy={busy}
              activeSessions={gameRunning}
              onCheckAppUpdate={checkAppUpdate}
              onInstallAppUpdate={installAppUpdate}
              onSaveGlobalDefaults={() =>
                action(async () => {
                  await call("set_global_profile_options", {
                    options: globalProfileOptions,
                  });
                  setMessage("Global emulator defaults saved");
                })
              }
              onSaveGithubToken={(token) =>
                action(async () => {
                  await call("save_github_token", { token });
                  setGithubTokenInput("");
                  setGithubTokenAvailable(true);
                  setGithubTokenError(null);
                  setMessage(`GitHub token stored in ${
                    currentCompatibilityOS() === "windows"
                      ? "Windows Credential Manager"
                      : "the system credential store"
                  }`);
                })
              }
              onDeleteGithubToken={() =>
                action(async () => {
                  await call("delete_github_token");
                  setGithubTokenAvailable(false);
                  setGithubTokenError(null);
                  setMessage("Stored GitHub token removed");
                })
              }
              onUpdateIntervalChange={(value) => {
                setUpdateInterval(value);
                action(() =>
                  call("set_setting", { key: "update_interval", value }),
                );
              }}
              onDiscordPresenceEnabledChange={(enabled) => {
                setDiscordPresenceEnabled(enabled);
                action(() => call("set_setting", {
                  key: "discord_presence_enabled",
                  value: enabled ? "true" : "false",
                }));
              }}
              onAutoUpdateModeChange={(value) => {
                setAutoUpdateMode(value);
                action(() =>
                  call("set_setting", { key: "auto_update_mode", value }),
                );
              }}
              onRefreshStorage={refreshStorageReport}
              onCleanIncompleteBuilds={() =>
                cleanStorage(
                  "clean_incomplete_builds",
                  (bytes) => `Incomplete emulator installations cleared · ${formatBytes(bytes)} freed`,
                )
              }
              onClearCompilerCache={() =>
                cleanStorage(
                  "clean_compiler_cache",
                  (bytes) => `Compiler cache cleared · ${formatBytes(bytes)} freed`,
                )
              }
              onClearCommunityWorktrees={() =>
                cleanStorage(
                  "clean_community_worktrees",
                  (bytes) => `Temporary community files cleared · ${formatBytes(bytes)} freed`,
                )
              }
              onClearSessionLogs={() =>
                cleanStorage(
                  "clean_session_logs",
                  (bytes) => `Session logs cleared · ${formatBytes(bytes)} freed; session history was kept`,
                  true,
                )
              }
            />
          )}
          {launcherSession && !launcherDismissed && (
            <LauncherSessionModal
              session={launcherSession}
              game={launcherGame}
              metric={launcherMetric}
              live={launcherLive}
              status={launcherStatus}
              runtimeSeconds={launcherRuntime}
              performance={launcherPerformance}
              icon={launcherIcon}
              log={launcherLog}
              onDismiss={() => setLauncherDismissed(true)}
              onStop={() =>
                action(async () => {
                  const result = await call<string>("stop_game_session", {
                    sessionId: launcherSession.id,
                  });
                  setMessage(result);
                  refreshProcessMetricsNow();
                })
              }
              onBackToLibrary={() => {
                setLauncherDismissed(true);
                setPage("library");
                if (launcherGame) selectGameWithGuard(launcherGame.id);
              }}
              onOpenLogs={() =>
                action(async () => {
                  const text = await call<string>("read_log", {
                    path: launcherSession.log_path,
                  });
                  setSelectedLogPath(launcherSession.log_path);
                  setSelectedLog(text);
                  setPage("diagnostics");
                  setLauncherDismissed(true);
                })
              }
              onCopyLog={() => void copyToClipboard(launcherLog)}
              canSearchCommunityPRs={trackedRepos.some((repo) => repo.source_path)}
              onSearchCommunityPRs={() => {
                const searchRepo =
                  trackedRepos.find(
                    (repo) => repo.id === selectedRepo && repo.source_path,
                  ) || trackedRepos.find((repo) => repo.source_path);
                if (!searchRepo) {
                  setMessage("Clone a repository before searching its community pull requests");
                  return;
                }
                const searchTerm = (
                  launcherGame?.title_id || launcherGame?.title || launcherSession.title
                ).slice(0, 128);
                setSelectedRepo(searchRepo.id);
                setCommunityState("all");
                setCommunityPage(1);
                setCommunitySearch(searchTerm);
                setLauncherDismissed(true);
                setPage("community");
              }}
            />
          )}
          {controllerRemappingEnabled && controllerRemapOpen && (
            <ControllerRemapDialog
              mapping={controllerMapping}
              onSave={saveControllerMapping}
              onClose={() => setControllerRemapOpen(false)}
            />
          )}
          {steamGame && (
            <SteamShortcutDialog
              game={steamGame}
              builds={builds}
              repos={repos}
              selectedBuildId={steamBuildId}
              busy={steamBusy}
              onSelectBuild={setSteamBuildId}
              onAdd={() => void confirmAddGameToSteam()}
              onClose={() => { if (!steamBusy) setSteamGame(null); }}
            />
          )}
        </div>
      </main>
    </div>
  );
}
export class AppErrorBoundary extends React.Component<
  React.PropsWithChildren,
  { error: Error | null }
> {
  state = { error: null as Error | null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("KytyPS5 Studio renderer error", error, info.componentStack);
  }
  render() {
    return this.state.error ? (
      <main className="fatal-error">
        <div>
          <CircleAlert size={28} />
          <h1>Studio could not render this screen</h1>
          <p>
            {this.state.error.message ||
              "An unexpected interface error occurred."}
          </p>
          <button className="primary" onClick={() => window.location.reload()}>
            Reload Studio
          </button>
        </div>
      </main>
    ) : (
      this.props.children
    );
  }
}
