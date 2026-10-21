import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Settings } from "lucide-react";
import type { AppUpdateInfo, Build, ProfileCapabilities, Repo, StorageReport } from "../types/studio";
import { ProfileOptionFields } from "../components/settings/ProfileOptionFields";
import { formatBytes } from "../utils/format";
import { currentCompatibilityOS } from "../utils/compatibility";

const primaryEmulatorFlags = [
  "--screen-width",
  "--screen-height",
  "--fullscreen",
  "--gpu",
  "--present-mode",
  "--perf-overlay",
];

export type SettingsPageProps = {
  builds: Build[];
  repos: Repo[];
  selectedBuildId: number | null;
  onSelectedBuildChange: (buildId: number | null) => void;
  globalProfileCapabilities: ProfileCapabilities | null;
  globalProfileOptions: Record<string, string>;
  setGlobalProfileOptions: Dispatch<SetStateAction<Record<string, string>>>;
  globalUnsupported: boolean;
  gpuDevices: { index: number; name: string }[];
  githubTokenAvailable: boolean;
  githubTokenError?: string | null;
  githubTokenInput: string;
  setGithubTokenInput: Dispatch<SetStateAction<string>>;
  updateInterval: string;
  discordPresenceEnabled?: boolean;
  autoUpdateMode: "notify" | "build";
  storageReport: StorageReport | null;
  appUpdate?: AppUpdateInfo | null;
  appUpdateStatus?: string;
  appUpdateBusy?: boolean;
  busy: boolean;
  activeSessions: boolean;
  onSaveGlobalDefaults: () => void;
  onSaveGithubToken: (token: string) => void;
  onDeleteGithubToken: () => void;
  onUpdateIntervalChange: (value: string) => void;
  onDiscordPresenceEnabledChange?: (enabled: boolean) => void;
  onAutoUpdateModeChange: (value: "notify" | "build") => void;
  onRefreshStorage: () => void | Promise<void>;
  onCleanIncompleteBuilds: () => void | Promise<void>;
  onClearCompilerCache: () => void | Promise<void>;
  onClearCommunityWorktrees: () => void | Promise<void>;
  onClearSessionLogs: () => void | Promise<void>;
  onCheckAppUpdate?: () => void;
  onInstallAppUpdate?: () => void;
};

export function SettingsPage({
  builds,
  repos,
  selectedBuildId,
  onSelectedBuildChange,
  globalProfileCapabilities,
  globalProfileOptions,
  setGlobalProfileOptions,
  globalUnsupported,
  gpuDevices,
  githubTokenAvailable,
  githubTokenError,
  githubTokenInput,
  setGithubTokenInput,
  updateInterval,
  discordPresenceEnabled = true,
  autoUpdateMode,
  storageReport,
  appUpdate,
  appUpdateStatus,
  appUpdateBusy,
  busy,
  activeSessions,
  onSaveGlobalDefaults,
  onSaveGithubToken,
  onDeleteGithubToken,
  onUpdateIntervalChange,
  onDiscordPresenceEnabledChange,
  onAutoUpdateModeChange,
  onRefreshStorage,
  onCleanIncompleteBuilds,
  onClearCompilerCache,
  onClearCommunityWorktrees,
  onClearSessionLogs,
  onCheckAppUpdate,
  onInstallAppUpdate,
}: SettingsPageProps) {
  const selectedBuild = builds.find((build) => build.id === selectedBuildId) ?? null;
  const selectedRepo = selectedBuild
    ? repos.find((repo) => repo.id === selectedBuild.repo_id)
    : null;
  const reportedOptions = globalProfileCapabilities?.options ?? [];
  const primaryOptions = reportedOptions.filter((flag) =>
    primaryEmulatorFlags.includes(flag),
  );
  const additionalOptions = reportedOptions.filter(
    (flag) => !primaryEmulatorFlags.includes(flag),
  );
  const hostOS = currentCompatibilityOS();
  const storageDescription = hostOS === "windows"
    ? "Application data and cache use LocalAppData; configuration uses Roaming AppData."
    : hostOS === "linux"
      ? "Application-managed files follow XDG user directories."
      : "Application-managed files use the host operating system’s user directories.";
  const credentialStore = hostOS === "windows"
    ? "Windows Credential Manager"
    : hostOS === "linux"
      ? "Linux Secret Service"
      : "the host credential store";
  const [storageTask, setStorageTask] = useState<string | null>(null);
  const [installedVersion, setInstalledVersion] = useState("Loading…");
  const storageTaskActive = useRef(false);
  useEffect(() => {
    let mounted = true;
    void getVersion()
      .then((version) => {
        if (mounted) setInstalledVersion(version);
      })
      .catch(() => {
        if (mounted) setInstalledVersion("Unavailable");
      });
    return () => {
      mounted = false;
    };
  }, []);
  const runStorageTask = (name: string, task: () => void | Promise<void>) => {
    if (storageTaskActive.current) return;
    storageTaskActive.current = true;
    setStorageTask(name);
    void Promise.resolve()
      .then(task)
      .catch(() => {})
      .finally(() => {
        storageTaskActive.current = false;
        setStorageTask(null);
      });
  };
  return (
    <section className="page settings-page">
      <div className="panel settings-emulator-panel">
        <div className="settings-emulator-heading">
          <div>
            <h3>Emulator settings</h3>
            <p>Choose an installed build. Defaults apply to games assigned to that build.</p>
          </div>
          <label>
            Emulator
            <select
              aria-label="Emulator settings build"
              value={selectedBuildId ?? ""}
              onChange={(event) =>
                onSelectedBuildChange(
                  event.target.value ? Number(event.target.value) : null,
                )
              }
            >
              <option value="" disabled>
                {builds.length ? "Select an emulator build" : "No installed emulator builds"}
              </option>
              {builds.map((build) => {
                const repo = repos.find((item) => item.id === build.repo_id);
                const repository = repo ? `${repo.owner}/${repo.name}` : "Unknown repository";
                return (
                  <option value={build.id} key={build.id}>
                    {repository} · {build.build_type} · {build.commit_sha.slice(0, 8)}
                    {build.active ? " · Active" : ""}
                  </option>
                );
              })}
            </select>
          </label>
        </div>
        {selectedBuild && (
          <div className="settings-emulator-summary">
            <strong>{selectedRepo ? `${selectedRepo.owner}/${selectedRepo.name}` : "Selected emulator"}</strong>
            <span>
              {selectedBuild.build_type} build · {selectedBuild.commit_sha.slice(0, 8)}
              {selectedBuild.active ? " · currently active" : ""}
            </span>
          </div>
        )}
        {selectedBuild && globalProfileCapabilities && (
          <div className="settings-build-caption">
            <span>{globalProfileCapabilities.options.length} CLI options reported by this build</span>
            <span>Only reported options are available below</span>
          </div>
        )}
        {globalUnsupported && (
          <div className="diagnosis-note">
            Some saved defaults are not supported by the selected build. They will
            apply again when a compatible build is selected.{" "}
            <button
              className="text-button"
              onClick={() =>
                setGlobalProfileOptions(
                  Object.fromEntries(
                    Object.entries(globalProfileOptions).filter(([flag]) =>
                      globalProfileCapabilities!.options.includes(flag),
                    ),
                  ),
                )
              }
            >
              Clear unsupported defaults
            </button>
          </div>
        )}
        {globalProfileCapabilities && primaryOptions.length > 0 && (
          <div className="settings-primary-options">
            <h4>Common settings</h4>
            <ProfileOptionFields
              available={primaryOptions}
              options={globalProfileOptions}
              switchOptions={globalProfileCapabilities.switch_options || []}
              previewTitle="Emulator launch arguments"
              onChange={setGlobalProfileOptions}
              gpuDevices={gpuDevices}
              showUnavailable={false}
            />
          </div>
        )}
        {globalProfileCapabilities && additionalOptions.length > 0 && (
          <details className="settings-additional-options">
            <summary>More options reported by this build ({additionalOptions.length})</summary>
            <p className="hint">These options come from the selected emulator’s command-line help.</p>
            <ProfileOptionFields
              available={additionalOptions}
              options={globalProfileOptions}
              switchOptions={globalProfileCapabilities.switch_options || []}
              previewTitle="Emulator launch arguments"
              onChange={setGlobalProfileOptions}
              gpuDevices={gpuDevices}
              showUnavailable={false}
            />
          </details>
        )}
        {globalProfileCapabilities && reportedOptions.length === 0 && (
          <div className="settings-empty-options">
            <strong>No configurable options reported</strong>
            <span>This build does not advertise any settings Studio can edit.</span>
          </div>
        )}
        <div className="settings-save-row">
          <span>{selectedBuild ? "Changes apply to games assigned to this build." : "Select an installed build to edit its settings."}</span>
          <button
            className="primary"
            disabled={!globalProfileCapabilities || !selectedBuildId || busy}
            onClick={onSaveGlobalDefaults}
          >
            {busy ? "Saving…" : "Save settings"}
          </button>
        </div>
      </div>
      <div className="sectionhead">
        <div>
          <h3>Settings & storage</h3>
          <p>{storageDescription}</p>
        </div>
        <Settings />
      </div>
      <div className="panel">
        <h3>GitHub access</h3>
        <p>
          Anonymous access works within GitHub rate limits. A personal access
          token is stored in {credentialStore}, never in Studio's database.
        </p>
        <div className="kv">
          <span>Credential status</span>
          <strong>
            {githubTokenError
              ? "Credential store unavailable"
              : githubTokenAvailable
              ? "Secure token available"
              : "Anonymous API access"}
          </strong>
        </div>
        {githubTokenError && <p role="alert">{githubTokenError}</p>}
        <label>Personal access token</label>
        <input
          type="password"
          autoComplete="new-password"
          value={githubTokenInput}
          onChange={(e) => setGithubTokenInput(e.target.value)}
          placeholder="Paste token (optional)"
        />
        <div className="buttonrow">
          <button
            className="outline"
            disabled={!githubTokenInput.trim()}
            onClick={() => onSaveGithubToken(githubTokenInput)}
          >
            Save securely
          </button>
          <button
            className="outline"
            disabled={!githubTokenAvailable}
            onClick={onDeleteGithubToken}
          >
            Remove token
          </button>
        </div>
      </div>
      <div className="panel">
        <h3>Emulator update checks</h3>
        <p>
          Checks compare each tracked source and newest verified build with its
          upstream branch. Automatic builds use the Release configuration, wait
          until no emulator sessions are running, install and verify into a new
          versioned directory, retain previous builds, and never activate
          automatically.
        </p>
        <label>Repeat checks</label>
        <select
          value={updateInterval}
          onChange={(event) => onUpdateIntervalChange(event.target.value)}
        >
          <option value="0">Startup only</option>
          <option value="6">Every 6 hours</option>
          <option value="24">Every 24 hours</option>
          <option value="168">Weekly</option>
        </select>
        <label>Update behavior</label>
        <select
          value={autoUpdateMode}
          onChange={(event) =>
            onAutoUpdateModeChange(event.target.value as "notify" | "build")
          }
        >
          <option value="notify">Notify only</option>
          <option value="build">Automatically fetch and build updates</option>
        </select>
        <p className="hint">
          Automatic builds wait for active emulator sessions to end. You choose
          when to activate the verified build.
        </p>
      </div>
      <div className="panel">
        <h3>Storage management</h3>
        <p>
          Sizes include downloaded source repositories and Studio-managed build
          data. Source repositories and original game content are not cleanup
          targets here.
        </p>
        {storageReport?.categories.map((category) => (
          <div className="kv" key={category.name}>
            <span>
              {category.name}
              <small className="storage-path">{category.path}</small>
            </span>
            <strong>{formatBytes(category.size_bytes)}</strong>
          </div>
        ))}
        <div className="buttonrow">
          <button
            className="outline"
            disabled={busy || storageTask !== null}
            onClick={() => runStorageTask("refresh", onRefreshStorage)}
          >
            {storageTask === "refresh" ? "Refreshing sizes…" : "Refresh sizes"}
          </button>
          <button
            className="outline danger"
            disabled={
              busy || storageTask !== null ||
              !storageReport?.categories.find(
                (category) => category.name === "Incomplete build installs",
              )?.size_bytes
            }
            onClick={() => {
              const bytes = storageReport?.categories.find(
                (category) => category.name === "Incomplete build installs",
              )?.size_bytes || 0;
              if (
                window.confirm(
                  `Remove ${formatBytes(bytes)} of incomplete, unregistered emulator installations? Registered builds and installations containing a kyty_emulator executable will be kept.`,
                )
              )
                runStorageTask("incomplete", onCleanIncompleteBuilds);
            }}
          >
            {storageTask === "incomplete" ? "Clearing incomplete builds…" : "Clear incomplete builds"}
          </button>
          <button
            className="outline danger"
            disabled={
              busy || storageTask !== null ||
              !storageReport?.categories.find(
                (category) => category.name === "Compiler build cache",
              )?.size_bytes
            }
            onClick={() => {
              const bytes =
                storageReport?.categories.find(
                  (category) => category.name === "Compiler build cache",
                )?.size_bytes || 0;
              if (
                window.confirm(
                  `Remove ${formatBytes(bytes)} of rebuildable compiler cache? Installed emulator builds, repositories, games, profiles and logs are stored separately and will be kept.`,
                )
              )
                runStorageTask("compiler", onClearCompilerCache);
            }}
          >
            {storageTask === "compiler" ? "Clearing compiler cache…" : "Clear compiler cache"}
          </button>
          <button
            className="outline danger"
            disabled={
              busy || storageTask !== null || activeSessions ||
              !storageReport?.categories.find(
                (category) => category.name === "Community build worktrees",
              )?.size_bytes
            }
            onClick={() => {
              const bytes =
                storageReport?.categories.find(
                  (category) => category.name === "Community build worktrees",
                )?.size_bytes || 0;
              if (
                window.confirm(
                  `Remove ${formatBytes(bytes)} of temporary community build source checkouts? Installed builds, build history, repositories, games, profiles and logs will be kept.`,
                )
              )
                runStorageTask("community", onClearCommunityWorktrees);
            }}
          >
            {storageTask === "community" ? "Clearing temporary files…" : "Clear temporary community files"}
          </button>
          <button
            className="outline danger"
            disabled={
              busy || storageTask !== null || activeSessions ||
              !storageReport?.categories.find(
                (category) => category.name === "Session logs",
              )?.size_bytes
            }
            onClick={() => {
              const bytes =
                storageReport?.categories.find(
                  (category) => category.name === "Session logs",
                )?.size_bytes || 0;
              if (
                window.confirm(
                  `Delete ${formatBytes(bytes)} of session log files? Session history and outcomes will remain, but deleted logs cannot be opened or exported. Active emulator sessions must finish first.`,
                )
              )
                runStorageTask("logs", onClearSessionLogs);
            }}
          >
            {storageTask === "logs" ? "Clearing session logs…" : "Clear session logs"}
          </button>
        </div>
      </div>
      <div className="panel">
        <h3>Discord Rich Presence</h3>
        <p>
          Show KytyPS5 Studio while it is open and the current game with its
          session timer while an emulator is running.
        </p>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={discordPresenceEnabled}
            onChange={(event) => onDiscordPresenceEnabledChange?.(event.target.checked)}
          />
          <span>Show activity on Discord</span>
        </label>
        <small className="hint">
          Presence is sent only to a Discord desktop client running on this
          computer. Discord shows the application icon; game artwork remains
          local to Studio.
        </small>
      </div>
      <div className="panel">
        <h3>KytyPS5 Studio updates</h3>
        <p>
          {appUpdateStatus || "App updates are checked automatically when Studio starts."}
        </p>
        {appUpdate?.update_available && (
          <p className="hint">
            Version {appUpdate.latest_version} is available
            {appUpdate.asset_name ? ` · ${appUpdate.asset_name}` : ""}.
            {appUpdate.asset_name?.toLowerCase().endsWith(".exe")
              ? " The verified NSIS installer opens only after you choose to install it."
              : appUpdate.running_appimage
              ? " The verified AppImage will replace this one after download. Restart Studio to use it."
              : " In-place installation is available when Studio runs from an AppImage."}
          </p>
        )}
        <button className="outline" disabled={appUpdateBusy} onClick={() => onCheckAppUpdate?.()}>
          {appUpdateBusy ? "Working…" : "Check for App Update"}
        </button>
        {appUpdate?.update_available && appUpdate.asset_name && (
          appUpdate.asset_name.toLowerCase().endsWith(".exe") || appUpdate.running_appimage ? (
            <button
              className="primary"
              disabled={appUpdateBusy || (appUpdate.asset_name.toLowerCase().endsWith(".exe") && (activeSessions || busy))}
              onClick={() => onInstallAppUpdate?.()}
            >
              {appUpdate.asset_name.toLowerCase().endsWith(".exe") ? "Install Verified Update" : "Install Verified AppImage Update"}
            </button>
          ) : null
        )}
        {(activeSessions || busy) && appUpdate?.asset_name?.toLowerCase().endsWith(".exe") && (
          <p className="hint">Finish active builds and close emulator sessions before installing the Studio update.</p>
        )}
      </div>
      <div className="panel">
        <h3>About</h3>
        <p>KytyPS5 Studio {installedVersion} · Cross-platform desktop</p>
        <p>
          Emulator source and games are supplied separately by the user. KytyPS5
          is GPL-2.0 licensed. Studio does not include firmware or games.
        </p>
      </div>
    </section>
  );
}
