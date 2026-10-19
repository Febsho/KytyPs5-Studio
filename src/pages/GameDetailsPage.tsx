import type { ReactNode, Dispatch, SetStateAction } from "react";
import { Play } from "lucide-react";
import type {
  Build,
  Game,
  GameCompatibility,
  ManualCompatibilityObservation,
  Repo,
  Session,
} from "../types/studio";
import { date, formatBytes, short } from "../utils/format";

type Tab = "Overview" | "Settings" | "Compatibility" | "Logs";
export type GameDetailsPageProps = {
  game: Game;
  tab: Tab;
  setTab: Dispatch<SetStateAction<Tab>>;
  profileDirty: boolean;
  builds: Build[];
  repos: Repo[];
  gameProfileEditor: ReactNode;
  gameCompatibility: GameCompatibility | null;
  manualOutcome: string;
  setManualOutcome: Dispatch<SetStateAction<string>>;
  manualNotes: string;
  setManualNotes: Dispatch<SetStateAction<string>>;
  compatibilityObservations: ManualCompatibilityObservation[];
  sessions: Session[];
  onBack: () => void;
  onLaunch: (gameId: number) => void;
  onOpenGameFolder: () => void;
  onRefreshMetadata: () => void;
  onRemoveEntry: () => void;
  onSetPreferredBuild: (buildId: number | null) => void;
  onRecordObservation: () => void;
  onOpenSessionLog: (session: Session) => void;
};

export function GameDetailsPage({
  game: currentGame,
  tab: gameDetailsTab,
  setTab: setGameDetailsTab,
  profileDirty,
  builds,
  repos,
  gameProfileEditor,
  gameCompatibility,
  manualOutcome,
  setManualOutcome,
  manualNotes,
  setManualNotes,
  compatibilityObservations,
  sessions,
  onBack,
  onLaunch,
  onOpenGameFolder,
  onRefreshMetadata,
  onRemoveEntry,
  onSetPreferredBuild,
  onRecordObservation,
  onOpenSessionLog,
}: GameDetailsPageProps) {
  return (
    <section className="page game-details-page">
      <div className="game-details-header">
        <button className="outline" onClick={onBack}>
          ← Library
        </button>
        <div>
          <p className="eyebrow">GAME DETAILS</p>
          <h1>{currentGame.title}</h1>
          <p className="path">{currentGame.path}</p>
        </div>
        <div className="buttonrow">
          <button
            className="outline"
            onClick={() => setGameDetailsTab("Settings")}
          >
            Settings
          </button>
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
        </div>
      </div>
      <nav className="details-tabs" aria-label="Game details tabs">
        {(["Overview", "Settings", "Compatibility", "Logs"] as const).map(
          (tab) => (
            <button
              key={tab}
              className={gameDetailsTab === tab ? "active" : ""}
              onClick={() => setGameDetailsTab(tab)}
            >
              {tab}
            </button>
          ),
        )}
      </nav>
      {gameDetailsTab === "Overview" && (
        <div className="details-overview">
          <div className="panel">
            <h2>Game overview</h2>
            <div className="game-meta">
              {currentGame.title_id && <span>{currentGame.title_id}</span>}
              {currentGame.game_version && (
                <span>Version {currentGame.game_version}</span>
              )}
              {currentGame.install_size !== null && (
                <span>{formatBytes(currentGame.install_size)}</span>
              )}
              <span>
                {currentGame.file_status === "Missing"
                  ? "Files missing"
                  : "Files available"}
              </span>
              <span>
                {Math.round(currentGame.play_seconds / 60)} min played
              </span>
            </div>
            <p>
              Game files remain at their registered location. Studio does not
              copy or modify the original game data.
            </p>
            <div className="buttonrow">
              <button
                className="outline"
                disabled={currentGame.file_status === "Missing"}
                onClick={onOpenGameFolder}
              >
                Open game folder
              </button>
              <button
                className="outline"
                disabled={currentGame.file_status === "Missing"}
                onClick={onRefreshMetadata}
              >
                Refresh metadata
              </button>
              <button
                className="outline danger"
                onClick={() => {
                  if (
                    profileDirty &&
                    !window.confirm("Discard unsaved game profile changes?")
                  )
                    return;
                  if (
                    window.confirm(
                      `Remove ${currentGame.title} from the library? The original files and session history will be kept.`,
                    )
                  )
                    onRemoveEntry();
                }}
              >
                Remove library entry
              </button>
              <label>
                Preferred emulator build
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
                      {repos.find((r) => r.id === b.repo_id)?.name} ·{" "}
                      {short(b.commit_sha)} · {b.build_type}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>
        </div>
      )}
      {gameDetailsTab === "Settings" && (
        <div className="game-profile-workspace">
          <div>
            <h2>Game settings</h2>
            <p>
              Only options advertised by this selected emulator build can be
              saved. Empty values inherit global defaults or KytyPS5 defaults.
            </p>
          </div>
          {gameProfileEditor}
        </div>
      )}
      {gameDetailsTab === "Compatibility" && (
        <div className="panel compatibility-panel">
          <h2>Compatibility evidence</h2>
          <div className="kv">
            <span>Supported path recognized</span>
            <strong>{gameCompatibility?.path_recognized ? "Yes" : "No"}</strong>
          </div>
          <div className="kv">
            <span>Emulator process starts</span>
            <strong>
              {gameCompatibility?.emulator_process_starts ?? "—"} /{" "}
              {gameCompatibility?.launch_attempts ?? "—"} attempts
            </strong>
          </div>
          <div className="kv">
            <span>Emulator reported title ID</span>
            <strong>
              {gameCompatibility?.emulator_reported_title_id
                ? "Observed"
                : "Not observed"}
            </strong>
          </div>
          <div className="kv">
            <span>Reached gameplay</span>
            <strong>{gameCompatibility?.gameplay_status || "Unknown"}</strong>
          </div>
          <div className="kv">
            <span>Confirmed playable</span>
            <strong>
              {gameCompatibility?.playable_status || "Unconfirmed"}
            </strong>
          </div>
          <p className="hint">
            Automatic process evidence is separate from these user-confirmed
            test notes. A clean process exit alone does not establish gameplay
            compatibility.
          </p>
          <h3>Your test observations</h3>
          <div className="formgrid">
            <div>
              <label htmlFor="compatibility-outcome">Outcome observed</label>
              <select
                id="compatibility-outcome"
                value={manualOutcome}
                onChange={(event) => setManualOutcome(event.target.value)}
              >
                <option value="launch_failed">Launch failed</option>
                <option value="reached_menu">Reached menu</option>
                <option value="reached_gameplay">Reached gameplay</option>
                <option value="playable">Playable</option>
                <option value="playable_with_issues">
                  Playable with issues
                </option>
              </select>
            </div>
            <div>
              <label htmlFor="compatibility-notes">Notes (optional)</label>
              <textarea
                id="compatibility-notes"
                rows={3}
                maxLength={2000}
                value={manualNotes}
                onChange={(event) => setManualNotes(event.target.value)}
                placeholder="Describe the test and any issues"
              />
            </div>
          </div>
          <button className="primary" onClick={onRecordObservation}>
            Record test observation
          </button>
          {compatibilityObservations.map((observation) => (
            <div className="compat-observation" key={observation.id}>
              <div className="kv">
                <span>
                  {observation.outcome.replaceAll("_", " ")} ·{" "}
                  {date(observation.observed_at)}
                </span>
                <strong>
                  {observation.repository} · {short(observation.build_commit)} ·{" "}
                  {observation.build_type}
                </strong>
              </div>
              {observation.notes && <p>{observation.notes}</p>}
              <details>
                <summary>Patch and compatibility settings</summary>
                <pre>{observation.patch_config_json}</pre>
              </details>
            </div>
          ))}
        </div>
      )}
      {gameDetailsTab === "Logs" && (
        <div className="panel">
          <h2>Recent session logs</h2>
          {sessions
            .filter((session) => session.game_id === currentGame.id)
            .map((session) => (
              <div className="kv" key={session.id}>
                <span>
                  {date(session.started_at)} · {session.outcome || "Running"}
                </span>
                <button
                  className="outline"
                  disabled={!session.log_path}
                  onClick={() => onOpenSessionLog(session)}
                >
                  {session.log_path ? "Open log" : "Log cleared"}
                </button>
              </div>
            ))}
          {!sessions.some((session) => session.game_id === currentGame.id) && (
            <p>No session logs yet.</p>
          )}
        </div>
      )}
    </section>
  );
}
