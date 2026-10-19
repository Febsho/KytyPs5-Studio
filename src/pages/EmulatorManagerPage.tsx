import { Check, ChevronRight, Cpu, Terminal } from "lucide-react";
import { useState } from "react";
import type { Build, Game, Repo, Session } from "../types/studio";
import { date, formatBytes, short } from "../utils/format";

export type EmulatorManagerPageProps = {
  builds: Build[];
  repos: Repo[];
  games: Game[];
  sessions: Session[];
  selectedLog: string;
  busy: boolean;
  onNewBuild: () => void;
  onOpenForkManager: () => void;
  onOpenBuildFolder: (id: number) => void;
  onVerifyBuild: (id: number) => void;
  onCheckUpdates: (repoId: number) => void;
  onRebuild: (build: Build) => void;
  onActivateBuild: (id: number) => void;
  onOpenBuildLog: (id: number) => void;
  onRemoveBuild: (id: number) => void;
  onCopyLog?: () => void;
};

export function EmulatorManagerPage({
  builds,
  repos,
  selectedLog,
  busy,
  onNewBuild,
  onOpenForkManager,
  onOpenBuildFolder,
  onVerifyBuild,
  onCheckUpdates,
  onRebuild,
  onActivateBuild,
  onOpenBuildLog,
  onRemoveBuild,
  onCopyLog,
}: EmulatorManagerPageProps) {
  const [compareIds, setCompareIds] = useState<number[]>([]);
  const comparison = compareIds
    .map((id) => builds.find((build) => build.id === id))
    .filter((build): build is Build => !!build);
  const toggleComparison = (id: number) => {
    setCompareIds((selected) => {
      if (selected.includes(id)) return selected.filter((value) => value !== id);
      return selected.length < 2 ? [...selected, id] : [selected[1], id];
    });
  };
  const details = [
    ["Repository", (build: Build) => {
      const repo = repos.find((item) => item.id === build.repo_id);
      return repo ? `${repo.name} · by ${repo.owner}` : "Unknown";
    }],
    ["Commit", (build: Build) => build.commit_sha],
    ["Build type", (build: Build) => build.build_type],
    ["Built", (build: Build) => date(build.created_at)],
    ["Compiler", (build: Build) => build.configuration.compiler || "Not recorded"],
    ["Parallel jobs", (build: Build) => build.configuration.jobs?.toString() || "Not recorded"],
    ["Clean build", (build: Build) => build.configuration.clean === null ? "Not recorded" : build.configuration.clean ? "Yes" : "No"],
    ["CMake configuration", (build: Build) => build.configuration.cmake_arguments?.join(" ") || "Not recorded"],
    ["Installation size", (build: Build) => formatBytes(build.install_size_bytes)],
    ["CLI verification", (build: Build) => `${build.executable_status} · ${build.validation_status}`],
    ["Last launch", (build: Build) => build.last_session_at ? `${build.last_session_outcome || "Unknown"}${build.last_session_exit_code !== null ? ` · exit ${build.last_session_exit_code}` : ""} · ${date(build.last_session_at)}` : "No launch recorded"],
    ["Installation path", (build: Build) => build.path],
  ] as const;
  return (
    <section className="page emulator-manager-page">
      <div className="sectionhead">
        <div>
          <h3>Installed emulator builds</h3>
          <p>
            Switch active builds without removing previous versions or changing
            game paths.
          </p>
        </div>
        <button className="outline" onClick={onNewBuild}>
          New build <ChevronRight size={16} />
        </button>
      </div>
      {builds.length ? (
        <div className="build-list">
          {builds.map((b) => (
            <div className="build-item" key={b.id}>
              <div className="build-icon">
                <Cpu />
              </div>
              <div>
                <strong>
                  {(() => {
                    const repo = repos.find((item) => item.id === b.repo_id);
                    return repo ? `${repo.name} · by ${repo.owner}` : "Repository";
                  })()} {" "}
                  <span className="subtle">{b.build_type}</span>
                </strong>
                <small>
                  Commit {short(b.commit_sha)} · Built {date(b.created_at)}
                  <br />
                  Size {formatBytes(b.install_size_bytes)} ·{" "}
                  {b.executable_status} · CLI {b.validation_status}
                  <br />
                  {b.last_session_at
                    ? `Last process result ${b.last_session_outcome || "Unknown"}${b.last_session_exit_code !== null ? ` · exit ${b.last_session_exit_code}` : ""} · ${date(b.last_session_at)}`
                    : "No launch sessions recorded"}
                  <br />
                  {b.path}
                </small>
                {!!b.community_prs?.length && (
                  <div className="build-community-prs">
                    <span className="badge">Built with community PRs</span>
                    {b.community_prs.map((pr) => (
                      <a key={`${pr.number}-${pr.head_sha}`} href={pr.url} target="_blank" rel="noreferrer">
                        #{pr.number} · {short(pr.head_sha)} ↗
                      </a>
                    ))}
                  </div>
                )}
                <details className="build-configuration">
                  <summary>Build configuration</summary>
                  {b.configuration.cmake_arguments.length ? (
                    <>
                      <p>
                        {b.configuration.compiler || "Compiler not recorded"} ·{" "}
                        {b.configuration.jobs ?? "?"} parallel jobs ·{" "}
                        {b.configuration.clean === null
                          ? "clean setting not recorded"
                          : b.configuration.clean
                            ? "clean build"
                            : "incremental build"}
                      </p>
                      <pre>{b.configuration.cmake_arguments.join("\n")}</pre>
                    </>
                  ) : (
                    <p>Configuration was not recorded for this existing build.</p>
                  )}
                </details>
              </div>
              <button
                className="outline"
                onClick={() => onOpenBuildFolder(b.id)}
              >
                Open folder
              </button>
              <button className="outline" onClick={() => onVerifyBuild(b.id)}>
                Verify CLI
              </button>
              <button
                className="outline"
                disabled={busy}
                onClick={() => onCheckUpdates(b.repo_id)}
              >
                Check updates
              </button>
              <button
                className="outline"
                disabled={
                  busy ||
                  !repos.find((repo) => repo.id === b.repo_id)?.source_path
                }
                onClick={() => onRebuild(b)}
              >
                Rebuild
              </button>
              {b.active ? (
                <span className="badge success">
                  <Check size={15} /> Active
                </span>
              ) : (
                <button
                  className="outline"
                  onClick={() => onActivateBuild(b.id)}
                >
                  Activate
                </button>
              )}
              <button className="outline" onClick={() => onOpenBuildLog(b.id)}>
                Log
              </button>
              <button
                className={compareIds.includes(b.id) ? "outline active" : "outline"}
                aria-pressed={compareIds.includes(b.id)}
                onClick={() => toggleComparison(b.id)}
              >
                {compareIds.includes(b.id) ? "Selected" : "Compare"}
              </button>
              <button
                className="outline danger"
                title="Remove this managed installation; game preferences will use the active build and session history will remain"
                onClick={() => {
                  onRemoveBuild(b.id);
                }}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div className="empty emulator-empty">
          <Cpu />
          <strong>No verified builds yet</strong>
          <p>Clone the official repository and compile it in Build Studio.</p>
          <button className="primary emulator-empty-action" onClick={onOpenForkManager}>
            Open Fork Manager
          </button>
        </div>
      )}
      {comparison.length > 0 && (
        <div className="panel build-comparison" aria-live="polite">
          <div className="sectionhead">
            <div>
              <h3>Build comparison</h3>
              <p>{comparison.length === 1 ? "Choose another build to compare its source, configuration, and validation." : "Build metadata and recorded configuration."}</p>
            </div>
            <button className="outline" onClick={() => setCompareIds([])}>Clear comparison</button>
          </div>
          {comparison.length === 2 && (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr><th scope="col">Property</th>{comparison.map((build) => {
                    const repo = repos.find((item) => item.id === build.repo_id);
                    return <th scope="col" key={build.id}>{repo ? `${repo.name} · by ${repo.owner}` : "Repository"} · {short(build.commit_sha)}</th>;
                  })}</tr>
                </thead>
                <tbody>
                  {details.map(([label, value]) => (
                    <tr key={label}>
                      <th scope="row">{label}</th>
                      {comparison.map((build) => <td key={build.id}>{value(build)}</td>)}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
      {selectedLog && (
        <div className="panel terminal">
          <div className="terminal-title">
            <Terminal size={16} /> Build log
            <button className="outline" onClick={() => onCopyLog?.()}>Copy log</button>
          </div>
          <pre>{selectedLog}</pre>
        </div>
      )}
    </section>
  );
}
