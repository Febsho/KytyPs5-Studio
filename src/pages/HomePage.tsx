import type { Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
import { Activity, ChevronRight, Play, RefreshCw } from "lucide-react";
import type {
  Build,
  Game,
  GameContent,
  Repo,
  OfficialCompatibilityIndex,
} from "../types/studio";
import { GameContextMenu, VirtualHomeGameRow } from "../components/games/GameLibrary";
import { GameCompatibilityBadge } from "../components/games/GameCompatibilityBadge";
import { emulatorBuildName, formatBytes } from "../utils/format";
import { trophySummary } from "../utils/trophies";

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
  featuredLogo?: string | null;
  featuredBackground: string | null;
  active?: Build;
  builds: Build[];
  repos: Repo[];
  message: string;
  setPage: Dispatch<SetStateAction<Page>>;
  choose: (kind: "file" | "directory") => Promise<void>;
  launchGame: (gameId: number) => Promise<void>;
  selectGameWithGuard: (gameId: number, afterSelect?: () => void) => boolean | void;
  setGameDetailsTab: Dispatch<
    SetStateAction<"Overview" | "Settings" | "Compatibility" | "Logs" | "Trophies" | "Patches" | "Save Data">
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
  featuredLogo,
  featuredBackground,
  active,
  builds,
  repos,
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
  const featuredBuild = builds.find((build) => build.id === featuredGame?.build_id) ?? active;
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
            officialCompatibilityIndex={officialCompatibilityIndex}
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
        {featuredLogo ? (
          <>
            <h2 className="sr-only">{featuredGame?.title || "KytyPS5 Studio"}</h2>
            <img className="home-feature-logo" src={featuredLogo} alt="" />
          </>
        ) : (
          <h2>{featuredGame?.title || "KytyPS5 Studio"}</h2>
        )}
        <div className="home-game-meta">
          {featuredGame?.title_id && <span>{featuredGame.title_id}</span>}
          {featuredGame && <GameCompatibilityBadge game={featuredGame} index={officialCompatibilityIndex} className="home-official-compatibility" />}
          {featuredGame?.game_version && <span>VERSION {featuredGame.game_version}</span>}
          {featuredGame?.install_size !== null && featuredGame?.install_size !== undefined && <span>{formatBytes(featuredGame.install_size)}</span>}
          {featuredGame?.last_played && <span>{Math.round(featuredGame.play_seconds / 60)} MIN PLAYED</span>}
          {trophyProgress && trophyProgress.total > 0 && <span>{trophyProgress.earned === null ? `${trophyProgress.total} TROPHIES` : `${trophyProgress.earned}/${trophyProgress.total} TROPHIES EARNED`}</span>}
        </div>
        {trophyProgress && trophyProgress.total > 0 && <div className="home-trophy-progress" aria-label="Trophy progress">
          <div className="trophy-progress-heading"><strong>Trophies</strong><span>{trophyProgress.earned === null ? "Earned status unavailable" : `${trophyProgress.earned} / ${trophyProgress.total} earned`}</span></div>
          <div className="trophy-progress-grid">{(["bronze", "silver", "gold", "platinum"] as const).map((grade) => <div className={`trophy-progress-grade trophy-${grade}`} key={grade}><span>{grade}</span><strong>{trophyProgress.earned === null ? "—" : trophyProgress.earnedByGrade[grade]} <i>/</i> {trophyProgress.counts[grade]}</strong></div>)}</div>
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
                  const openDetails = () => {
                    setGameDetailsTab("Overview");
                    setPage("game-details");
                  };
                  if (selectGameWithGuard(featuredGame.id, openDetails) !== false) openDetails();
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
        </div>
        <div className="home-status-line">
          <span>
            <i className="statusdot" />
            {featuredBuild
              ? emulatorBuildName(repos.find((repo) => repo.id === featuredBuild.repo_id), featuredBuild)
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
    </section>
  );
}
