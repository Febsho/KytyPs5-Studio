import type { Dispatch, SetStateAction } from "react";
import { Play, RefreshCw, Square, Terminal } from "lucide-react";
import type { BuildSourceInfo, Repo, System } from "../types/studio";
import { short } from "../utils/format";

export type BuildStudioPageProps = {
  repos: Repo[];
  currentRepo?: Repo;
  buildSourceInfo: BuildSourceInfo | null;
  system: System | null;
  buildMode: "simple" | "advanced";
  setBuildMode: Dispatch<SetStateAction<"simple" | "advanced">>;
  buildType: string;
  setBuildType: Dispatch<SetStateAction<string>>;
  jobs: number;
  setJobs: Dispatch<SetStateAction<number>>;
  cleanBuild: boolean;
  setCleanBuild: Dispatch<SetStateAction<boolean>>;
  busy: boolean;
  currentBuildPhase: string;
  elapsedSeconds: number;
  logs: string[];
  onSelectRepo: (id: number) => void;
  onBuild: () => void;
  onBuildLatest: () => void;
  onInstallTools: () => void;
  onCancel: () => void;
  onCopyLog?: () => void;
};

function buildStageIndex(phase: string) {
  const value = phase.toLowerCase();
  if (/complete|succeeded|verified/.test(value)) return 7;
  if (/fetch|preparing repository/.test(value)) return 0;
  if (/submodule|updating source/.test(value)) return 1;
  if (/dependenc/.test(value)) return 2;
  if (/configur/.test(value)) return 3;
  if (/compile|ninja/.test(value)) return 4;
  if (/install/.test(value)) return 5;
  if (/validat/.test(value)) return 6;
  return -1;
}

function buildStageClass(phase: string, index: number) {
  const active = buildStageIndex(phase);
  if (active < 0) return "";
  if (active > index) return "complete";
  return active === index ? "current" : "";
}

export function BuildStudioPage({
  repos,
  currentRepo,
  buildSourceInfo,
  system,
  buildMode,
  setBuildMode,
  buildType,
  setBuildType,
  jobs,
  setJobs,
  cleanBuild,
  setCleanBuild,
  busy,
  currentBuildPhase,
  elapsedSeconds,
  logs,
  onSelectRepo,
  onBuild,
  onBuildLatest,
  onInstallTools,
  onCancel,
  onCopyLog,
}: BuildStudioPageProps) {
  const onModeChange = setBuildMode;
  const onBuildTypeChange = setBuildType;
  const onJobsChange = setJobs;
  const onCleanBuildChange = setCleanBuild;
  const linuxBuildToolNames = new Set([
    "cmake", "ninja", "clang", "clang++", "pkg-config", "python3",
    "glslangValidator", "vulkan", "x11", "wayland-client", "libpulse", "libudev",
  ]);
  const isWindows = system?.os.toLowerCase().includes("windows") ?? false;
  const isLinux = system?.os.toLowerCase().includes("linux") ?? false;
  const missingBuildTools = system
    ? system.dependencies
        .filter((dependency) =>
          isWindows
            ? !dependency.present
            : isLinux && linuxBuildToolNames.has(dependency.name) && !dependency.present,
        )
        .map((dependency) => dependency.name)
    : [];
  const buildToolsMissing = missingBuildTools.length > 0;
  const canCancel = busy && currentBuildPhase.toLowerCase() !== "installing tools";
  return (
    <section className="page build-studio-page">
      <div className="studio-page-heading">
        <div>
          <small className="eyebrow">
            SYSTEM SOFTWARE / KYTYPS5 BUILD STUDIO
          </small>
          <h2>Build Studio</h2>
        </div>
        <div
          className="mode-switch"
          role="group"
          aria-label="Build interface mode"
        >
          <button
            className={buildMode === "simple" ? "selected" : ""}
            onClick={() => onModeChange("simple")}
          >
            Simple
          </button>
          <button
            className={buildMode === "advanced" ? "selected" : ""}
            onClick={() => onModeChange("advanced")}
          >
            Advanced
          </button>
        </div>
      </div>
      {buildToolsMissing ? (
        <div className="build-tool-install-notice" role="status">
          <span>Missing build tools: {missingBuildTools.join(", ")}.</span>
          <button className="outline" disabled={busy} onClick={onInstallTools}>
            Install missing tools
          </button>
        </div>
      ) : null}
      <div className="build-summary technical-summary">
        <div>
          <small>Repository</small>
          <strong>
            {currentRepo
              ? `${currentRepo.owner}/${currentRepo.name}`
              : "No repository selected"}
          </strong>
        </div>
        <div>
          <small>Source branch</small>
          <strong>
            {buildSourceInfo?.branch ||
              (currentRepo?.source_path
                ? "Inspecting checkout…"
                : "Not cloned")}
          </strong>
        </div>
        <div>
          <small>Build stage</small>
          <strong>{currentBuildPhase}</strong>
        </div>
        <div>
          <small>Elapsed</small>
          <strong>
            {Math.floor(elapsedSeconds / 60)}m{" "}
            {(elapsedSeconds % 60).toString().padStart(2, "0")}s
          </strong>
        </div>
      </div>
      {buildMode === "simple" ? (
        <div className="build-simple-layout">
          <div className="build-config-surface">
            <label>Emulator repository</label>
            <select
              value={currentRepo?.id ?? ""}
              onChange={(e) => onSelectRepo(Number(e.target.value))}
            >
              {repos.map((repo) => (
                <option key={repo.id} value={repo.id}>
                  {repo.owner}/{repo.name}
                </option>
              ))}
            </select>
            <div className="build-source-row">
              <div className="build-source-mark">
                {currentRepo?.name.slice(0, 1).toUpperCase() || "K"}
              </div>
              <div>
                <strong>
                  {currentRepo
                    ? `${currentRepo.owner}/${currentRepo.name}`
                    : "No repository selected"}
                </strong>
                <small>
                  {currentRepo?.branch || "Source is not cloned"} ·{" "}
                  {currentRepo?.adapter || "Adapter unavailable"}
                </small>
              </div>
            </div>
            <div className="technical-row">
              <span>Checkout</span>
              <strong>
                {!currentRepo?.source_path
                  ? "Clone required"
                  : !buildSourceInfo
                    ? "Inspecting source…"
                    : buildSourceInfo.working_tree_status.length
                      ? `${buildSourceInfo.working_tree_status.length} working-tree changes`
                      : "Clean working tree"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Checked out commit</span>
              <strong>{short(buildSourceInfo?.commit_sha || null)}</strong>
            </div>
            <div className="technical-row">
              <span>Build adapter</span>
              <strong>
                {["official", "kyty-cmake"].includes(currentRepo?.adapter || "")
                  ? "Verified KytyPS5 CMake"
                  : "Unsupported / unverified"}
              </strong>
            </div>
            <div className="formgrid">
              <div>
                <label>Configuration</label>
                <select
                  value={buildType}
                  disabled={busy}
                  onChange={(e) => onBuildTypeChange(e.target.value)}
                >
                  <option>Release</option>
                  <option>Debug</option>
                  <option>RelWithDebInfo</option>
                </select>
              </div>
              <div>
                <label>Ninja parallel jobs</label>
                <input
                  type="number"
                  min="1"
                  max="128"
                  value={jobs}
                  disabled={busy}
                  onChange={(e) =>
                    onJobsChange(
                      Math.max(1, Math.min(128, Number(e.target.value) || 1)),
                    )
                  }
                />
              </div>
            </div>
            <div className="build-ready-row">
              <span
                className={`statusdot ${currentRepo?.source_path ? "" : "muted"}`}
              />
              {currentRepo?.source_path
                ? `${system ? `${system.dependencies.filter((dependency) => dependency.present).length} / ${system.dependencies.length} checked tools found` : "Checking system dependencies…"}`
                : "Select and clone a source repository"}
            </div>
            <button
              className="primary build-main-action"
              disabled={
                busy ||
                buildToolsMissing ||
                !currentRepo?.source_path ||
                !["official", "kyty-cmake"].includes(currentRepo.adapter)
              }
              onClick={onBuild}
            >
              <Play size={16} /> Configure & Build
            </button>
          </div>
          <div className="build-progress-column">
            <div className="pipeline-steps">
              {[
                "Fetching source",
                "Updating submodules",
                "Checking dependencies",
                "Configuring CMake",
                "Compiling with Ninja",
                "Installing build",
                "Validating executable",
              ].map((stage, index) => (
                <div
                  className={buildStageClass(currentBuildPhase, index)}
                  key={stage}
                >
                  <span>{String(index + 1).padStart(2, "0")}</span>
                  {stage}
                </div>
              ))}
            </div>
            <div className="panel terminal">
              <div className="terminal-title">
                <Terminal size={15} /> Build output{" "}
                <span>{busy ? "RUNNING" : "IDLE"}</span>
                <button className="outline" onClick={() => onCopyLog?.()} disabled={!logs.length}>Copy log</button>
              </div>
              <pre>
                {logs.length
                  ? logs.join("\n")
                  : "Build logs will appear here when the build starts."}
              </pre>
              <div className="build-action-row">
                <button
                  className="outline"
                  disabled={
                    busy ||
                    buildToolsMissing ||
                    !currentRepo?.source_path ||
                    !["official", "kyty-cmake"].includes(currentRepo.adapter)
                  }
                  onClick={onBuildLatest}
                >
                  <RefreshCw size={14} /> Fetch & Build Latest
                </button>
                <button
                  className="outline danger-action"
                  disabled={!canCancel}
                  onClick={onCancel}
                >
                  <Square size={13} /> Cancel
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : (
        <div className="build-advanced-layout">
          <div className="build-config-surface advanced-config">
            <label>Repository</label>
            <select
              value={currentRepo?.id ?? ""}
              onChange={(e) => onSelectRepo(Number(e.target.value))}
            >
              {repos.map((repo) => (
                <option key={repo.id} value={repo.id}>
                  {repo.owner}/{repo.name}
                </option>
              ))}
            </select>
            <div className="technical-row">
              <span>Source directory</span>
              <strong>{currentRepo?.source_path || "Not cloned"}</strong>
            </div>
            <div className="technical-row">
              <span>Git branch</span>
              <strong>
                {buildSourceInfo?.branch ||
                  (currentRepo?.source_path
                    ? "Inspecting checkout…"
                    : "Not cloned")}
              </strong>
            </div>
            <div className="technical-row">
              <span>Checked out commit</span>
              <strong>{short(buildSourceInfo?.commit_sha || null)}</strong>
            </div>
            <div className="technical-row">
              <span>Working tree</span>
              <strong>
                {!buildSourceInfo
                  ? "Inspecting…"
                  : buildSourceInfo.working_tree_status.length
                    ? `${buildSourceInfo.working_tree_status.length} local changes`
                    : "Clean"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Submodule workflow</span>
              <strong>
                {!currentRepo?.source_path
                  ? "Unavailable until clone"
                  : !buildSourceInfo
                    ? "Inspecting…"
                    : buildSourceInfo.submodules.length
                      ? buildSourceInfo.submodules
                          .map((line) =>
                            line.trim()[0] === "-"
                              ? "Uninitialized"
                              : line.trim()[0] === "+"
                                ? "Changed from pinned commit"
                                : line.trim()[0] === "U"
                                  ? "Conflict"
                                  : "Initialized",
                          )
                          .join(", ")
                      : "No submodules"}
              </strong>
            </div>
            {buildSourceInfo?.submodules.length ? (
              <details className="submodule-list">
                <summary>
                  Submodule commit status ({buildSourceInfo.submodules.length})
                </summary>
                {buildSourceInfo.submodules.map((line) => (
                  <code key={line}>{line}</code>
                ))}
              </details>
            ) : null}
            <div className="formgrid">
              <div>
                <label>Build type</label>
                <select
                  value={buildType}
                  disabled={busy}
                  onChange={(e) => onBuildTypeChange(e.target.value)}
                >
                  <option>Release</option>
                  <option>Debug</option>
                  <option>RelWithDebInfo</option>
                </select>
              </div>
              <div>
                <label>Compiler</label>
                <input value="Clang / Clang++ · adapter requirement" readOnly />
              </div>
              <div>
                <label>Ninja jobs</label>
                <input
                  type="number"
                  min="1"
                  max="128"
                  value={jobs}
                  disabled={busy}
                  onChange={(e) =>
                    onJobsChange(
                      Math.max(1, Math.min(128, Number(e.target.value) || 1)),
                    )
                  }
                />
              </div>
              <div>
                <label>CMake / Ninja output</label>
                <input
                  value={
                    buildSourceInfo?.build_directory ||
                    "Inspecting build directory…"
                  }
                  readOnly
                />
              </div>
            </div>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={cleanBuild}
                disabled={busy}
                onChange={(e) => onCleanBuildChange(e.target.checked)}
              />
              Clean CMake output
            </label>
            <div className="technical-row">
              <span>Verbose output</span>
              <strong>Live compiler output enabled</strong>
            </div>
            <div className="technical-row cmake-arguments">
              <span>CMake arguments</span>
              <strong>
                {buildSourceInfo?.cmake_arguments.join(" · ") ||
                  "Inspecting actual configure arguments…"}
              </strong>
            </div>
            <div className="technical-row">
              <span>Install root</span>
              <strong>{system?.paths.builds || "Unavailable"}</strong>
            </div>
            <div className="build-action-row">
              <button
                className="primary"
                disabled={
                  busy ||
                  buildToolsMissing ||
                  !currentRepo?.source_path ||
                  !["official", "kyty-cmake"].includes(currentRepo.adapter)
                }
                onClick={onBuild}
              >
                <Play size={15} /> Configure & Build
              </button>
              <button
                className="outline danger-action"
                disabled={!canCancel}
                onClick={onCancel}
              >
                <Square size={13} /> Cancel
              </button>
            </div>
          </div>
          <div className="terminal advanced-terminal">
            <div className="pipeline-steps compact-pipeline">
              {[
                "Fetching",
                "Submodules",
                "Dependencies",
                "CMake configure",
                "Ninja compile",
                "Install",
                "Validate",
              ].map((stage, index) => (
                <div
                  className={buildStageClass(currentBuildPhase, index)}
                  key={stage}
                >
                  <span>{String(index + 1).padStart(2, "0")}</span>
                  {stage}
                </div>
              ))}
            </div>
            <div className="terminal-title">
              <Terminal size={15} /> Compiler output{" "}
              <span>{busy ? "RUNNING" : "IDLE"}</span>
              <button className="outline" onClick={() => onCopyLog?.()} disabled={!logs.length}>Copy log</button>
            </div>
            <pre>
              {logs.length
                ? logs.join("\n")
                : "Detailed compiler and CMake output appears here."}
            </pre>
          </div>
        </div>
      )}
    </section>
  );
}
