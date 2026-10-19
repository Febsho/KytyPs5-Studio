import { Gamepad2, Square, Terminal, X } from "lucide-react";
import type { Game, ProcessMetric, Session } from "../../types/studio";
import { short } from "../../utils/format";

export type LauncherSessionModalProps = {
  session: Session;
  game?: Game;
  metric?: ProcessMetric;
  live: boolean;
  status: string;
  runtimeSeconds: number;
  icon: string | null;
  log: string;
  onDismiss: () => void;
  onStop: () => void;
  onBackToLibrary: () => void;
  onOpenLogs: () => void;
};

export function LauncherSessionModal({
  session: launcherSession,
  game: launcherGame,
  metric: launcherMetric,
  live: launcherLive,
  status: launcherStatus,
  runtimeSeconds: launcherRuntime,
  icon: launcherIcon,
  log: launcherLog,
  onDismiss,
  onStop,
  onBackToLibrary,
  onOpenLogs,
}: LauncherSessionModalProps) {
  return (
    <div
      className="launcher-overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Emulator session"
    >
      <section className="launcher-modal">
        <header>
          <span>
            <i className={`launcher-live-dot ${launcherLive ? "" : "ended"}`} />{" "}
            KYTYPS5 LAUNCHER
          </span>
          <button
            className="launcher-close"
            onClick={onDismiss}
            aria-label="Dismiss launcher"
          >
            <X size={15} />
          </button>
        </header>
        <div className="launcher-main">
          <div className="launcher-cover">
            {launcherIcon ? (
              <img src={launcherIcon} alt="" />
            ) : (
              <Gamepad2 size={42} />
            )}
          </div>
          <div className="launcher-summary">
            <h2>{launcherSession.title}</h2>
            <small>
              {launcherGame?.title_id || "TITLE ID UNKNOWN"} ·{" "}
              {launcherSession.build_type} · {short(launcherSession.commit_sha)}
            </small>
            <div className={`launcher-running ${launcherLive ? "" : "ended"}`}>
              <i
                className={`launcher-live-dot ${launcherLive ? "" : "ended"}`}
              />
              <strong>
                {launcherLive
                  ? launcherMetric?.status === "Starting"
                    ? "Starting game"
                    : "Game running"
                  : launcherStatus}
              </strong>
            </div>
            <p>
              Session {launcherSession.id} · Elapsed{" "}
              {Math.floor(launcherRuntime / 60)}m{" "}
              {(launcherRuntime % 60).toString().padStart(2, "0")}s
            </p>
            <div className="launcher-actions">
              <button
                className="stop-game"
                disabled={!launcherLive || !launcherMetric?.pid}
                onClick={onStop}
              >
                <Square size={13} fill="currentColor" /> Stop Game
              </button>
              <button className="outline" onClick={onBackToLibrary}>
                Back to Library
              </button>
            </div>
          </div>
        </div>
        <details className="launcher-details" open>
          <summary>⌁ &nbsp; Technical Details</summary>
          <div className="launcher-metrics">
            <div>
              <small>PROCESS ID</small>
              <strong>
                {launcherMetric?.pid ?? launcherSession.pid ?? "Unavailable"}
              </strong>
            </div>
            <div>
              <small>EXIT STATUS</small>
              <strong>{launcherStatus}</strong>
            </div>
            <div>
              <small>CPU</small>
              <strong>
                {launcherLive &&
                launcherMetric?.cpu_percent !== null &&
                launcherMetric?.cpu_percent !== undefined
                  ? `${launcherMetric.cpu_percent.toFixed(1)}%`
                  : "—"}
              </strong>
            </div>
            <div>
              <small>MEMORY</small>
              <strong>
                {launcherLive &&
                launcherMetric?.resident_bytes !== null &&
                launcherMetric?.resident_bytes !== undefined
                  ? `${(launcherMetric.resident_bytes / 1024 / 1024).toFixed(0)} MiB`
                  : "—"}
              </strong>
            </div>
          </div>
          <div className="launcher-log-head">
            <span>PROCESS OUTPUT & EMULATOR LOG</span>
            <button
              className="outline"
              title={launcherSession.log_path || "Session log unavailable"}
              disabled={!launcherSession?.log_path}
              onClick={onOpenLogs}
            >
              <Terminal size={13} /> Open Logs
            </button>
          </div>
          <p className="launcher-log-path">
            {launcherSession.log_path ||
              (launcherLive
                ? "Waiting for the session log path"
                : "Session log was cleared")}
          </p>
          <pre className="launcher-log">
            {launcherLog || "Waiting for emulator output…"}
          </pre>
        </details>
      </section>
    </div>
  );
}
