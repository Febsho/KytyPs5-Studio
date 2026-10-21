import type { Dispatch, SetStateAction } from "react";
import { Activity, ChevronRight, Terminal } from "lucide-react";
import type {
  GraphicsDiagnostics,
  ProcessMetric,
  Session,
  System,
} from "../types/studio";
import { date } from "../utils/format";
import { analyzeEmulatorLog, buildCrashReport } from "../utils/crashAnalyzer";

export type DiagnosticsPageProps = {
  system: System | null;
  graphics: GraphicsDiagnostics | null;
  processMetrics: ProcessMetric[];
  sessions: Session[];
  selectedLog: string;
  selectedLogPath: string | null;
  logFilter: "All" | "Info" | "Warning" | "Error" | "Debug";
  setLogFilter: Dispatch<
    SetStateAction<"All" | "Info" | "Warning" | "Error" | "Debug">
  >;
  onSelectSessionLog: (session: Session) => void;
  onExportLog: () => void;
  onExportCrashReport: (report: string) => void;
  onCopyLog?: () => void;
};

function logLineLevel(line: string): "Info" | "Warning" | "Error" | "Debug" {
  if (/\b(debug|trace)\b/i.test(line)) return "Debug";
  if (/\b(warn|warning)\b/i.test(line)) return "Warning";
  if (/\b(error|exception|fault|crash|unresolved|failed|fatal)\b/i.test(line))
    return "Error";
  return "Info";
}

export function DiagnosticsPage({
  system,
  graphics,
  processMetrics,
  sessions,
  selectedLog,
  selectedLogPath,
  logFilter,
  setLogFilter,
  onSelectSessionLog,
  onExportLog,
  onExportCrashReport,
  onCopyLog,
}: DiagnosticsPageProps) {
  const selectedSession = sessions.find((session) => session.log_path === selectedLogPath);
  const findings = selectedLog ? analyzeEmulatorLog(selectedLog) : [];
  const crashReport = selectedSession
    ? buildCrashReport(selectedLog, {
        game: selectedSession.title,
        build: `${selectedSession.build_type} · ${selectedSession.commit_sha}`,
        startedAt: date(selectedSession.started_at),
      })
    : "";
  return (
    <section className="page">
      <div className="sectionhead">
        <div>
          <h3>System diagnostics</h3>
          <p>Observed host details and actual emulator session results.</p>
        </div>
        <Activity />
      </div>
      <div className="diag-grid">
        <div className="panel">
          <h3>Host system</h3>
          <div className="kv">
            <span>OS</span>
            <strong>{system?.os}</strong>
          </div>
          <div className="kv">
            <span>Architecture</span>
            <strong>{system?.arch}</strong>
          </div>
          <div className="kv">
            <span>Processor</span>
            <strong>{system?.cpu}</strong>
          </div>
          <div className="kv">
            <span>Memory</span>
            <strong>
              {system
                ? `${(system.memory_available_bytes / 1024 ** 3).toFixed(1)} / ${(system.memory_total_bytes / 1024 ** 3).toFixed(1)} GB available`
                : undefined}
            </strong>
          </div>
          <div className="kv">
            <span>Display session</span>
            <strong>{system?.session}</strong>
          </div>
          <div className="kv">
            <span>Graphics device</span>
            <strong>{system?.gpu}</strong>
          </div>
        </div>
        <div className="panel">
          <h3>Graphics runtime</h3>
          <div className="kv">
            <span>Vulkan</span>
            <strong
              className={
                graphics?.vulkan_status === "Available" ? "good" : "bad"
              }
            >
              {graphics?.vulkan_status || "Checking…"}
            </strong>
          </div>
          <div className="kv">
            <span>Graphics driver</span>
            <strong>{graphics?.drivers.join(", ") || "Unavailable"}</strong>
          </div>
          <div className="kv">
            <span>Connected displays</span>
            <strong>{graphics?.monitors.join("; ") || "None detected"}</strong>
          </div>
          <pre className="graphics-details">
            {graphics?.vulkan_details ||
              "Probing the system Vulkan loader and installed ICDs…"}
          </pre>
        </div>
        <div className="panel">
          <h3>Toolchain</h3>
          <div className="deps">
            {system?.dependencies.map((d) => (
              <div key={d.name}>
                <span>{d.name}</span>
                <span className={d.present ? "good" : "bad"}>
                  {d.present ? "Found" : "Missing"}
                </span>
              </div>
            ))}
          </div>
        </div>
        <div className="panel">
          <h3>Detected controllers</h3>
          {system?.controllers.length ? (
            <>
              <div className="deps">
                {system.controllers.map((controller) => (
                  <div key={controller}>
                    <span>{controller}</span>
                    <span className="good">Detected</span>
                  </div>
                ))}
              </div>
              <p className="hint">
                Host detection does not guarantee game compatibility.
              </p>
            </>
          ) : (
            <p className="hint">
              No supported game controllers were detected by host diagnostics.
            </p>
          )}
        </div>
      </div>
      <div className="panel process-panel">
        <h3>Live emulator processes</h3>
        {processMetrics.length ? (
          <div className="process-list">
            {processMetrics.map((process) => (
              <div className="process-row" key={process.session_id}>
                <div>
                  <strong>{process.title}</strong>
                  <small>
                    PID {process.pid ?? "Starting"} ·{" "}
                    {Math.floor(process.runtime_seconds / 60)}m{" "}
                    {process.runtime_seconds % 60}s · {process.status}
                  </small>
                </div>
                <div className="process-usage">
                  <span>
                    {process.cpu_percent === null
                      ? "—"
                      : `${process.cpu_percent.toFixed(1)}% CPU`}
                  </span>
                  <span>
                    {process.resident_bytes === null
                      ? "—"
                      : `${(process.resident_bytes / 1024 / 1024).toFixed(0)} MiB RAM`}
                  </span>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p className="hint">No emulator processes are currently running.</p>
        )}
      </div>
      <div className="sectionhead">
        <h3>Recent sessions</h3>
      </div>
      {sessions.length ? (
        <div className="session-list">
          {sessions.slice(0, 10).map((s) => (
            <div className="session-entry" key={s.id}>
            <button aria-expanded={selectedLogPath === s.log_path && !!selectedLog} onClick={() => onSelectSessionLog(s)}>
              <div>
                <strong>{s.title}</strong>
                <small>
                  {date(s.started_at)} · {s.outcome || "Running"} · exit{" "}
                  {s.exit_code ?? "—"}
                </small>
              </div>
              <ChevronRight size={18} />
            </button>
            {selectedLogPath === s.log_path && selectedLog && <div className="panel terminal session-inline-log">
              <div className="terminal-title"><Terminal size={16} /> Session log
                <button className="outline" onClick={() => onCopyLog?.()}>Copy log</button>
                {selectedLogPath && <button className="outline" onClick={onExportLog}>Export log</button>}
                <select value={logFilter} onChange={(e) => setLogFilter(e.target.value as typeof logFilter)}>
                  <option>All</option><option>Info</option><option>Warning</option><option>Error</option><option>Debug</option>
                </select>
              </div>
              <div className="crash-analyzer">
                <div className="crash-analyzer-heading">
                  <div>
                    <h4>Smart Crash Analyzer</h4>
                    <p>Log signatures are grouped as investigation hints; they do not confirm a root cause.</p>
                  </div>
                  <button
                    className="outline"
                    disabled={!crashReport}
                    onClick={() => onExportCrashReport(crashReport)}
                  >
                    Export GitHub report
                  </button>
                </div>
                {findings.length ? (
                  <div className="crash-finding-list">
                    {findings.map((finding) => (
                      <article className="crash-finding" key={finding.id}>
                        <strong>{finding.title}</strong>
                        <p>{finding.summary}</p>
                        <small>{finding.nextStep}</small>
                        <ul>
                          {finding.evidence.map((line, index) => <li key={`${finding.id}-${index}`}><code>{line}</code></li>)}
                        </ul>
                      </article>
                    ))}
                  </div>
                ) : (
                  <p className="hint">No known failure signature matched this log. The cause is undetermined.</p>
                )}
              </div>
              <pre>{selectedLog.split("\n").filter((line) => logFilter === "All" || logLineLevel(line) === logFilter).join("\n") || `No ${logFilter.toLowerCase()} lines in this log.`}</pre>
            </div>}
            </div>
          ))}
        </div>
      ) : (
        <div className="empty compact">
          <Activity />
          <strong>No sessions recorded</strong>
        </div>
      )}
    </section>
  );
}
