import type { Dispatch, SetStateAction } from "react";
import { useEffect, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import {
  BookOpen,
  FolderOpen,
  Gamepad2,
  Heart,
  Play,
  Search,
} from "lucide-react";
import type {
  Build,
  Game,
  GameContent,
  GameScanEvent,
  OfficialCompatibilityIndex,
  Repo,
} from "../types/studio";
import { GameContextMenu, VirtualGameGrid } from "../components/games/GameLibrary";
import { GameCompatibilityBadge } from "../components/games/GameCompatibilityBadge";
import { emulatorBuildName } from "../utils/format";
import { trophySummary } from "../utils/trophies";
import { compatibilityBadgeLabel, officialCompatibilityForGame } from "../utils/compatibility";

export type GameLibraryPageProps = {
  games: Game[];
  filteredGames: Game[];
  currentGame?: Game;
  gameBackground: string | null;
  gameIcon: string | null;
  gameScan: GameScanEvent | null;
  officialCompatibilityIndex?: OfficialCompatibilityIndex | null;
  inspectGameContent?: (gameId: number, includeIcons?: boolean) => Promise<GameContent>;
  query: string;
  setQuery: Dispatch<SetStateAction<string>>;
  libraryFilter: "all" | "favorites" | "recent";
  setLibraryFilter: Dispatch<SetStateAction<"all" | "favorites" | "recent">>;
  librarySort: "title" | "recent" | "playtime" | "size";
  setLibrarySort: Dispatch<
    SetStateAction<"title" | "recent" | "playtime" | "size">
  >;
  libraryView: "grid" | "list";
  setLibraryView: Dispatch<SetStateAction<"grid" | "list">>;
  builds: Build[];
  repos: Repo[];
  profileDirty: boolean;
  onChoose: (kind: "file" | "directory") => Promise<void>;
  onSelectGame: (id: number) => boolean | void;
  onOpenDetails: () => void;
  onOpenGameFolder: () => void;
  onOpenFolderForGame?: (game: Game) => void;
  onLaunch: (id: number) => void;
  onToggleFavorite: (id: number) => void;
  onSetPreferredBuild: (id: number | null) => void;
  onRemoveGame?: (id: number) => void;
};

export function GameLibraryPage({
  games,
  filteredGames,
  currentGame,
  gameBackground,
  gameIcon,
  gameScan,
  officialCompatibilityIndex,
  inspectGameContent,
  query,
  setQuery,
  libraryFilter,
  setLibraryFilter,
  librarySort,
  setLibrarySort,
  libraryView,
  setLibraryView,
  builds,
  repos,
  profileDirty,
  onChoose,
  onSelectGame,
  onOpenDetails,
  onOpenGameFolder,
  onOpenFolderForGame,
  onLaunch,
  onToggleFavorite,
  onSetPreferredBuild,
  onRemoveGame,
}: GameLibraryPageProps) {
  const [contextMenu, setContextMenu] = useState<{ game: Game; x: number; y: number } | null>(null);
  const [trophyProgress, setTrophyProgress] = useState<ReturnType<typeof trophySummary> | null>(null);
  const activeBuildId = builds.find((build) => build.active)?.id;
  const compatibility = currentGame ? officialCompatibilityForGame(currentGame, officialCompatibilityIndex ?? null) : null;
  const reportCount = compatibility && Object.keys(compatibility.report_counts).length
    ? Object.values(compatibility.report_counts).reduce((sum, count) => sum + count, 0)
    : null;
  useEffect(() => {
    if (!currentGame || !inspectGameContent) { setTrophyProgress(null); return; }
    let current = true;
    setTrophyProgress(null);
    const timer = window.setTimeout(() => {
      void inspectGameContent(currentGame.id, false).then((content) => { if (current) setTrophyProgress(trophySummary(content)); }).catch(() => { if (current) setTrophyProgress(null); });
    }, 800);
    return () => { current = false; window.clearTimeout(timer); };
  }, [currentGame?.id, currentGame?.build_id, activeBuildId, currentGame?.last_played, inspectGameContent]);
  return (
    <section
      className="page library-page"
      style={
        gameBackground
          ? {
              backgroundImage: `linear-gradient(90deg,#050609ed 0%,#050609ad 52%,#050609d9 100%),linear-gradient(0deg,#050609c9,transparent 70%),url("${gameBackground}")`,
            }
          : undefined
      }
    >
      <div className="sectionhead">
        <div>
          <h3>Game library</h3>
          <p>Original files stay at their existing paths.</p>
        </div>
        <div className="buttonrow">
          <button className="outline" onClick={() => onChoose("file")}>
            <FolderOpen size={16} /> Add file
          </button>
          <button
            className="primary"
            disabled={!!gameScan && !gameScan.done}
            onClick={() => onChoose("directory")}
          >
            <FolderOpen size={16} />{" "}
            {gameScan && !gameScan.done ? "Scanning…" : "Scan folder"}
          </button>
        </div>
      </div>
      {gameScan && !gameScan.done && (
        <div className="scan-progress" role="status">
          <strong>{gameScan.phase}</strong>
          <span>
            {gameScan.phase === "Scanning folder"
              ? `${gameScan.completed} entries checked`
              : gameScan.total
                ? `${gameScan.completed} / ${gameScan.total} titles sized`
                : "Preparing title list"}
          </span>
        </div>
      )}
      {games.length > 0 && <div className="library-toolbar">
        <div className="search">
          <Search size={18} />
          <input
            aria-label="Search your library"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search title, title ID, or path"
          />
        </div>
        <select
          aria-label="Library filter"
          value={libraryFilter}
          onChange={(e) =>
            setLibraryFilter(e.target.value as typeof libraryFilter)
          }
        >
          <option value="all">All games</option>
          <option value="favorites">Favorites</option>
          <option value="recent">Recently played</option>
        </select>
        <select
          aria-label="Sort games"
          value={librarySort}
          onChange={(e) => setLibrarySort(e.target.value as typeof librarySort)}
        >
          <option value="title">Sort: Title</option>
          <option value="recent">Sort: Recently played</option>
          <option value="playtime">Sort: Playtime</option>
          <option value="size">Sort: Install size</option>
        </select>
        <div className="buttonrow" aria-label="Library layout">
          <button
            className={libraryView === "grid" ? "outline active" : "outline"}
            aria-pressed={libraryView === "grid"}
            onClick={() => setLibraryView("grid")}
          >
            Grid
          </button>
          <button
            className={libraryView === "list" ? "outline active" : "outline"}
            aria-pressed={libraryView === "list"}
            onClick={() => setLibraryView("list")}
          >
            List
          </button>
        </div>
      </div>}
      {filteredGames.length > 0 && <div className="library-layout">
        <VirtualGameGrid
          games={filteredGames}
          selectedId={currentGame?.id}
          onSelect={onSelectGame}
          officialCompatibilityIndex={officialCompatibilityIndex}
          onContextMenu={(game, event: ReactMouseEvent<HTMLButtonElement>) => {
            event.preventDefault();
            setContextMenu({ game, x: event.clientX, y: event.clientY });
          }}
          mode={libraryView}
        />
        {currentGame && filteredGames.some((game) => game.id === currentGame.id) && (
          <div className="panel detail">
            <div className="detail-art">
              {gameBackground && (
                <img
                  className="detail-background"
                  src={gameBackground}
                  alt=""
                />
              )}
              {gameIcon ? (
                <img
                  className="detail-icon"
                  src={gameIcon}
                  alt={`${currentGame.title} icon`}
                />
              ) : (
                !gameBackground && <Gamepad2 size={56} />
              )}
            </div>
            <div className="library-title-row">
              <h2>{currentGame.title}</h2>
              <GameCompatibilityBadge game={currentGame} index={officialCompatibilityIndex} />
            </div>
            {compatibility && <p className="library-compatibility-context">
              Official compatibility: {compatibilityBadgeLabel(compatibility)}
              {reportCount !== null && <> · {reportCount} {reportCount === 1 ? "report" : "reports"}</>}
              {compatibility.latest_tested && <> · Latest test {compatibility.latest_tested}</>}
            </p>}
            <p className="path">{currentGame.path}</p>
            <div className="library-detail-actions">
              <button
                className="outline"
                disabled={currentGame.file_status === "Missing"}
                onClick={onOpenGameFolder}
              >
                Open game folder
              </button>
              <button className="outline" onClick={onOpenDetails}>
                Game details
              </button>
            </div>
            <div className="game-meta">
              {currentGame.title_id && <span>{currentGame.title_id}</span>}
              {currentGame.game_version && (
                <span>Version {currentGame.game_version}</span>
              )}
              {currentGame.install_size !== null && (
                <span>
                  {(currentGame.install_size / 1024 / 1024 / 1024).toFixed(1)}{" "}
                  GB
                </span>
              )}
            </div>
            {trophyProgress && trophyProgress.total > 0 && <div className="library-trophy-progress">
              <div className="trophy-progress-heading"><strong>Trophies</strong><span>{trophyProgress.earned === null ? "Earned status unavailable" : `${trophyProgress.earned} / ${trophyProgress.total} earned`}</span></div>
              <div className="trophy-progress-grid">{(["bronze", "silver", "gold", "platinum"] as const).map((grade) => <div className={`trophy-progress-grade trophy-${grade}`} key={grade}><span>{grade}</span><strong>{trophyProgress.earned === null ? "—" : trophyProgress.earnedByGrade[grade]} <i>/</i> {trophyProgress.counts[grade]}</strong></div>)}</div>
            </div>}
            <div className="buttonrow">
              <button
                className="primary"
                disabled={currentGame.file_status === "Missing"}
                title={
                  currentGame.file_status === "Missing"
                    ? "Registered game path is missing"
                    : ""
                }
                onClick={() => onLaunch(currentGame.id)}
              >
                <Play size={17} /> Play game
              </button>
              <button
                className="outline"
                onClick={() => onToggleFavorite(currentGame.id)}
              >
                <Heart
                  size={17}
                  fill={currentGame.favorite ? "currentColor" : "none"}
                />
              </button>
            </div>
            <label>Preferred emulator build</label>
            <select
              value={currentGame.build_id ?? ""}
              onChange={(event) => {
                if (
                  profileDirty &&
                  !window.confirm(
                    "This game profile has unsaved changes. Discard them and switch emulator builds?",
                  )
                )
                  return;
                onSetPreferredBuild(
                  event.target.value ? Number(event.target.value) : null,
                );
              }}
            >
              <option value="">Use active build</option>
              {builds.map((b) => (
                <option value={b.id} key={b.id}>
                  {emulatorBuildName(repos.find((r) => r.id === b.repo_id), b)}
                </option>
              ))}
            </select>
            <div className="detail-stat">
              Playtime{" "}
              <strong>{Math.round(currentGame.play_seconds / 60)} min</strong>
            </div>
          </div>
        )}
      </div>}
      {contextMenu && (
        <GameContextMenu
          {...contextMenu}
          onClose={() => setContextMenu(null)}
          onDetails={(game) => {
            setContextMenu(null);
            if (onSelectGame(game.id) !== false) onOpenDetails();
          }}
          onOpenFolder={onOpenFolderForGame ? (game) => {
            setContextMenu(null);
            onOpenFolderForGame(game);
          } : undefined}
          onRemove={onRemoveGame ? (game) => {
            setContextMenu(null);
            onRemoveGame(game.id);
          } : undefined}
        />
      )}
      {games.length === 0 && (
        <div className="empty library-empty">
          <BookOpen />
          <strong>Your library is empty</strong>
          <p>Import a supported game folder to add it to your library.</p>
          <button className="primary" onClick={() => void onChoose("directory")}>
            <FolderOpen size={15} /> Import game folder
          </button>
        </div>
      )}
      {games.length > 0 && filteredGames.length === 0 && (
        <div className="empty library-empty">
          <Search />
          <strong>No games match these filters</strong>
          <p>Change the search or library filter to see more games.</p>
        </div>
      )}
    </section>
  );
}
