import type { Dispatch, SetStateAction } from "react";
import { Activity, ChevronRight, Play, RefreshCw } from "lucide-react";
import type {
  Build,
  Game,
  GraphicsDiagnostics,
  Repo,
  System,
} from "../types/studio";
import { HomeGameTile } from "../components/games/GameLibrary";
import { date, formatBytes, short } from "../utils/format";

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
  selectGameWithGuard: (gameId: number) => void;
  setGameDetailsTab: Dispatch<
    SetStateAction<"Overview" | "Settings" | "Compatibility" | "Logs">
  >;
  setHomeSelection: Dispatch<SetStateAction<number | null>>;
};

export function HomePage({
  games,
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
}: HomePageProps) {
  return (
    <section
      className="page home-page"
      style={
        featuredBackground
          ? {
              backgroundImage: `linear-gradient(180deg,rgba(4,5,8,.44) 0%,rgba(4,5,8,.22) 32%,rgba(4,5,8,.82) 72%,#050609 100%),linear-gradient(90deg,rgba(4,5,8,.75),transparent 70%),url(${featuredBackground})`,
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
          <div className="home-game-row">
            {games.slice(0, 8).map((game) => (
              <HomeGameTile
                key={game.id}
                game={game}
                selected={featuredGame?.id === game.id}
                onSelect={() => setHomeSelection(game.id)}
              />
            ))}
          </div>
        ) : (
          <div className="home-empty">
            <span>No imported games</span>
            <button
              className="outline"
              onClick={() => void choose("directory")}
            >
              Import game folder
            </button>
          </div>
        )}
      </div>
      <div className="home-feature">
        <div className="home-game-meta">
          {featuredGame?.title_id && <span>{featuredGame.title_id}</span>}
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
        </div>
        <h2>{featuredGame?.title || "KytyPS5 Studio"}</h2>
        <p>
          {featuredGame
            ? featuredGame.last_played
              ? `Last played ${date(featuredGame.last_played)}.`
              : "Imported game · gameplay compatibility is unconfirmed."
            : "Import a supported game folder to build your library."}
        </p>
        <div className="hero-actions">
          {featuredGame ? (
            <>
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
          <span>{system?.gpu || "Graphics device unavailable"}</span>
          <span>
            {system?.controllers.length
              ? `${system.controllers.length} controller${system.controllers.length === 1 ? "" : "s"} detected`
              : "No controller detected"}
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
      </div>
      <div className="home-recent">
        <div className="sectionhead">
          <div>
            <small className="eyebrow">LIBRARY</small>
            <h3>Recently played</h3>
          </div>
          <button className="text-button" onClick={() => setPage("library")}>
            All games <ChevronRight size={14} />
          </button>
        </div>
        {games.filter((game) => game.last_played).length ? (
          <div className="recent-strip">
            {[...games]
              .filter((game) => game.last_played)
              .sort((a, b) => (b.last_played ?? 0) - (a.last_played ?? 0))
              .slice(0, 4)
              .map((game) => (
                <button key={game.id} onClick={() => setHomeSelection(game.id)}>
                  <span>{game.title}</span>
                  <small>{date(game.last_played)}</small>
                </button>
              ))}
          </div>
        ) : (
          <p className="recent-empty">No play sessions recorded yet.</p>
        )}
      </div>
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
