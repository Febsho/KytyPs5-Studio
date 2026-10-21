import type { Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
import { Activity, ChevronRight, Play, RefreshCw } from "lucide-react";
import type {
  Build,
  Game,
  GameContent,
  GraphicsDiagnostics,
  Repo,
  System,
  OfficialCompatibilityIndex,
} from "../types/studio";
import { GameContextMenu, VirtualHomeGameRow } from "../components/games/GameLibrary";
import { date, formatBytes, short } from "../utils/format";
import { trophySummary } from "../utils/trophies";
import { compatibilityBadgeLabel, officialCompatibilityForGame } from "../utils/compatibility";

type Page =
  | "home"
  | "library"
  | "forks"
  | "community"
  | "build"
  | "emulators"
  | "diagnostics"
  | "settings"
  | "game-details";
export type HomePageProps = {
  games: Game[];
  officialCompatibilityIndex?: OfficialCompatibilityIndex | null;
  featuredGame: Game | null;
  featuredBackground: string | null;
  active?: Build;
  repos: Repo[];
  system: System | null;
  graphics: GraphicsDiagnostics | null;
  specsOpen: boolean;
  setSpecsOpen: Dispatch<SetStateAction<boolean>>;
  message: string;
  setPage: Dispatch<SetStateAction<Page>>;
  choose: (kind: "file" | "directory") => Promise<void>;
  launchGame: (gameId: number) => Promise<void>;
  selectGameWithGuard: (gameId: number) => boolean | void;
  setGameDetailsTab: Dispatch<
    SetStateAction<"Overview" | "Settings" | "Compatibility" | "Logs" | "Trophies" | "Cheats">
  >;
  setHomeSelection: Dispatch<SetStateAction<number | null>>;
  inspectGameContent?: (gameId:number) => Promise<GameContent>;
  onRemoveGame?: (gameId: number) => void;
  onAddToSteam?: (game: Game) => void;
  runningGameId?: number;
  onShowRunningSession?: () => void;
};

export function HomePage({
  games,
  officialCompatibilityIndex,
  featuredGame,
  featuredBackground,
  active,
  repos,
  system,
  graphics,
  specsOpen,
  setSpecsOpen,
  message,
  setPage,
  choose,
  launchGame,
  selectGameWithGuard,
  setGameDetailsTab,
  setHomeSelection,
  inspectGameContent,
  onRemoveGame,
  onAddToSteam,
  runningGameId,
  onShowRunningSession,
}: HomePageProps) {
  const [trophyProgress, setTrophyProgress] = useState<ReturnType<typeof trophySummary> | null>(null);
  const [contextMenu, setContextMenu] = useState<{ game: Game; x: number; y: number } | null>(null);
  useEffect(() => {
    if (!featuredGame || !inspectGameContent) { setTrophyProgress(null); return; }
    let current = true;
    setTrophyProgress(null);
    const timer = window.setTimeout(() => {
      void inspectGameContent(featuredGame.id).then((result)=>{ if(current) setTrophyProgress(trophySummary(result)); }).catch(()=>{ if(current) setTrophyProgress(null); });
    }, 1200);
    return ()=>{ current=false; window.clearTimeout(timer); };
  }, [featuredGame?.id, inspectGameContent]);
  return (
    <section
      className="page home-page"
      style={
        featuredBackground
          ? {
              backgroundImage: `linear-gradient(180deg,rgba(2,3,5,.76) 0%,rgba(2,3,5,.5) 25%,rgba(2,3,5,.36) 48%,rgba(2,3,5,.86) 76%,#050609 100%),linear-gradient(90deg,rgba(2,3,5,.8),transparent 72%),url(${featuredBackground})`,
            }
          : undefined
      }
    >
      <div className="home-shelf">
        <div className="shelf-heading">
          <span>YOUR GAMES</span>
          <span>{games.length ? `(${games.length})` : "(0)"}</span>
          <button className="text-button" onClick={() => setPage("library")}>
            Library <ChevronRight size={14} />
          </button>
        </div>
        {games.length ? (
          <VirtualHomeGameRow
            games={games}
            selectedId={featuredGame?.id}
            onSelect={setHomeSelection}
            onActivate={(gameId) => void launchGame(gameId)}
            onContextMenu={(selected, event) => {
              event.preventDefault();
              setContextMenu({ game: selected, x: event.clientX, y: event.clientY });
            }}
          />
        ) : (
          <div className="home-empty">
            <button
              className="primary"
              onClick={() => void choose("directory")}
            >
              Import game folder
            </button>
          </div>
        )}
      </div>
      {featuredGame && <div className="home-feature">
        <div className="home-game-meta">
          {featuredGame?.title_id && <span>{featuredGame.title_id}</span>}
          {featuredGame && officialCompatibilityIndex && officialCompatibilityForGame(featuredGame, officialCompatibilityIndex) && <span className="home-official-compatibility">{compatibilityBadgeLabel(officialCompatibilityForGame(featuredGame, officialCompatibilityIndex))}</span>}
          {featuredGame?.game_version && (
            <span>VERSION {featuredGame.game_version}</span>
          )}
          {featuredGame?.install_size !== null &&
            featuredGame?.install_size !== undefined && (
              <span>{formatBytes(featuredGame.install_size)}</span>
            )}
          {featuredGame?.last_played && (
            <span>{Math.round(featuredGame.play_seconds / 60)} MIN PLAYED</span>
          )}
          {trophyProgress && trophyProgress.total > 0 && <span>{trophyProgress.earned === null ? `${trophyProgress.total} TROPHIES` : `${trophyProgress.earned}/${trophyProgress.total} TROPHIES EARNED`}</span>}
        </div>
        <h2>{featuredGame?.title || "KytyPS5 Studio"}</h2>
        <p>
          {featuredGame
            ? featuredGame.last_played
              ? `Last played ${date(featuredGame.last_played)}.`
              : "Imported game · gameplay compatibility is unconfirmed."
            : "Import a supported game folder to build your library."}
        </p>
        {trophyProgress?.earned !== null && trophyProgress?.earned !== undefined && trophyProgress.total > 0 && <div className="home-trophy-progress" aria-label="Trophy progress">
          <span>{trophyProgress.earned} of {trophyProgress.total} earned</span>
          {(["platinum", "gold", "silver", "bronze"] as const).filter((grade) => trophyProgress.counts[grade] > 0).map((grade) => <span className={`trophy-progress-grade trophy-${grade}`} key={grade}>{trophyProgress.earnedByGrade[grade]}/{trophyProgress.counts[grade]} {grade}</span>)}
        </div>}
        <div className="hero-actions">
          {featuredGame ? (
            <>
              {runningGameId === featuredGame.id ? (
                <button className="primary" onClick={onShowRunningSession}>
                  <Activity size={15} /> Open running session
                </button>
              ) : (
                <button
                  className="primary"
                  disabled={featuredGame.file_status === "Missing"}
                  title={
                    featuredGame.file_status === "Missing"
                      ? "Registered game path is missing"
                      : ""
                  }
                  onClick={() => void launchGame(featuredGame.id)}
                >
                  <Play size={15} fill="currentColor" /> Play Game
                </button>
              )}
              <button
                className="ghost"
                onClick={() => {
                  selectGameWithGuard(featuredGame.id);
                  setGameDetailsTab("Overview");
                  setPage("game-details");
                }}
              >
                Game Details
              </button>
            </>
          ) : (
            <button
              className="primary"
              onClick={() => void choose("directory")}
            >
              Import Game Folder
            </button>
          )}
          <button className="spec-link" onClick={() => setSpecsOpen(true)}>
            <Activity size={14} /> Technical Specs
          </button>
        </div>
        <div className="home-status-line">
          <span>
            <i className="statusdot" />
            {active
              ? `${repos.find((repo) => repo.id === active.repo_id)?.name || "KytyPS5"} · ${short(active.commit_sha)}`
              : "No verified emulator build"}
          </span>
          {message.includes("Update available:") && (
            <button
              className="home-update-link"
              onClick={() => setPage("emulators")}
            >
              <RefreshCw size={12} /> Emulator update available · Review builds
            </button>
          )}
        </div>
      </div>}
      {contextMenu && (
        <GameContextMenu
          {...contextMenu}
          onClose={() => setContextMenu(null)}
          onDetails={(game) => {
            setContextMenu(null);
            if (selectGameWithGuard(game.id) === false) return;
            setGameDetailsTab("Overview");
            setPage("game-details");
          }}
          onAddToSteam={(game) => {
            setContextMenu(null);
            onAddToSteam?.(game);
          }}
          onRemove={onRemoveGame ? (game) => {
            setContextMenu(null);
            onRemoveGame(game.id);
          } : undefined}
        />
      )}
      {specsOpen && (
        <div className="drawer-backdrop" onClick={() => setSpecsOpen(false)}>
          <aside
            className="spec-drawer"
            onClick={(event) => event.stopPropagation()}
          >
            <button
              className="drawer-close outline"
              onClick={() => setSpecsOpen(false)}
            >
              Close
            </button>
            <small className="eyebrow">SYSTEM / TECHNICAL SPECIFICATIONS</small>
            <h2>System Status</h2>
            <div className="technical-row">
              <span>Operating system</span>
              <strong>
                {system?.os || "Unavailable"} · {system?.arch || "—"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Processor</span>
              <strong>{system?.cpu || "Unavailable"}</strong>
            </div>
            <div className="technical-row">
              <span>Graphics device</span>
              <strong>{system?.gpu || "Unavailable"}</strong>
            </div>
            <div className="technical-row">
              <span>Vulkan</span>
              <strong>{graphics?.vulkan_status || "Not checked"}</strong>
            </div>
            <div className="technical-row">
              <span>Graphics driver</span>
              <strong>{graphics?.drivers.join(", ") || "Not detected"}</strong>
            </div>
            <div className="technical-row">
              <span>Memory available</span>
              <strong>
                {system
                  ? `${(system.memory_available_bytes / 1024 ** 3).toFixed(1)} / ${(system.memory_total_bytes / 1024 ** 3).toFixed(1)} GB`
                  : "Unavailable"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Active emulator</span>
              <strong>
                {active
                  ? `${short(active.commit_sha)} · ${active.build_type}`
                  : "None"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Display session</span>
              <strong>{system?.session || "Unavailable"}</strong>
            </div>
            <button
              className="outline"
              onClick={() => {
                setSpecsOpen(false);
                setPage("diagnostics");
              }}
            >
              Open Diagnostics
            </button>
          </aside>
        </div>
      )}
    </section>
  );
}
