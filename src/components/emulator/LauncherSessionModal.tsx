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
  performance?: {
    averageCpuPercent: number | null;
    peakCpuPercent: number | null;
    peakResidentBytes: number | null;
  };
  icon: string | null;
  log: string;
  onDismiss: () => void;
  onStop: () => void;
  onBackToLibrary: () => void;
  onOpenLogs: () => void;
  onCopyLog?: () => void;
  onSearchCommunityPRs?: () => void;
  canSearchCommunityPRs?: boolean;
};

export function LauncherSessionModal({
  session: launcherSession,
  game: launcherGame,
  metric: launcherMetric,
  live: launcherLive,
  status: launcherStatus,
  runtimeSeconds: launcherRuntime,
  performance: launcherPerformance = {
    averageCpuPercent: null,
    peakCpuPercent: null,
    peakResidentBytes: null,
  },
  icon: launcherIcon,
  log: launcherLog,
  onDismiss,
  onStop,
  onBackToLibrary,
  onOpenLogs,
  onCopyLog,
  onSearchCommunityPRs,
  canSearchCommunityPRs = false,
}: LauncherSessionModalProps) {
  const sessionFailed = !launcherLive && (
    (launcherSession.exit_code !== null && launcherSession.exit_code !== 0) ||
    /error|crash|failed/i.test(launcherSession.outcome || "")
  );
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
              {launcherLive ? "KYTYPS5 LAUNCHER" : "SESSION SUMMARY"}
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
              {sessionFailed && onSearchCommunityPRs && (
                <button
                  className="outline"
                  disabled={!canSearchCommunityPRs}
                  title={canSearchCommunityPRs ? "Search pull requests in a cloned repository" : "Clone a repository to search its community pull requests"}
                  onClick={onSearchCommunityPRs}
                >
                  Search community PRs
                </button>
              )}
            </div>
          </div>
        </div>
        <details className="launcher-details" open>
          <summary>{launcherLive ? "⌁  Technical Details" : "⌁  Session overview and performance"}</summary>
          {launcherLive ? <div className="launcher-metrics">
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
          </div> : <>
            <div className="launcher-metrics session-summary-metrics">
              <div><small>PLAYTIME</small><strong>{Math.floor(launcherRuntime / 60)}m {(launcherRuntime % 60).toString().padStart(2, "0")}s</strong></div>
              <div><small>RESULT</small><strong>{launcherStatus}</strong></div>
              <div><small>EXIT CODE</small><strong>{launcherSession.exit_code ?? "Unavailable"}</strong></div>
              <div><small>AVERAGE CPU</small><strong>{launcherPerformance.averageCpuPercent === null ? "Unavailable" : `${launcherPerformance.averageCpuPercent.toFixed(1)}%`}</strong></div>
              <div><small>PEAK CPU</small><strong>{launcherPerformance.peakCpuPercent === null ? "Unavailable" : `${launcherPerformance.peakCpuPercent.toFixed(1)}%`}</strong></div>
              <div><small>PEAK MEMORY</small><strong>{launcherPerformance.peakResidentBytes === null ? "Unavailable" : `${(launcherPerformance.peakResidentBytes / 1024 / 1024).toFixed(0)} MiB`}</strong></div>
              <div><small>AVERAGE FPS</small><strong>Not measured</strong></div>
              <div><small>1% LOW</small><strong>Not measured</strong></div>
              <div><small>PEAK FPS</small><strong>Not measured</strong></div>
            </div>
            <p className="launcher-metric-note">The emulator does not currently expose frame-time telemetry, so FPS and 1% low values are unavailable.</p>
          </>}
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
            <button className="outline" disabled={!launcherLog} onClick={() => onCopyLog?.()}>Copy log</button>
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
