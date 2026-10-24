import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { Database, Gamepad2, Github, RefreshCw, Settings } from "lucide-react";
import type { AppUpdateInfo, Build, ProfileCapabilities, Repo, StorageReport } from "../types/studio";
import { ProfileOptionFields } from "../components/settings/ProfileOptionFields";
import { emulatorBuildName, formatBytes } from "../utils/format";
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
  discordPresenceEnabled?: boolean;
  storageReport: StorageReport | null;
  appUpdate?: AppUpdateInfo | null;
  appUpdateStatus?: string;
  appUpdateBusy?: boolean;
  busy: boolean;
  activeSessions: boolean;
  onSaveGlobalDefaults: () => void | Promise<void>;
  onSaveGithubToken: (token: string) => void;
  onDeleteGithubToken: () => void;
  onDiscordPresenceEnabledChange?: (enabled: boolean) => void;
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
  discordPresenceEnabled = true,
  storageReport,
  appUpdate,
  appUpdateStatus,
  appUpdateBusy,
  busy,
  activeSessions,
  onSaveGlobalDefaults,
  onSaveGithubToken,
  onDeleteGithubToken,
  onDiscordPresenceEnabledChange,
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
  const isOfficialKytyPs5 =
    selectedRepo?.owner.toLowerCase() === "kytyps5" &&
    selectedRepo.name.toLowerCase() === "kytyps5";
  const supportsBackgroundShaderPrecompile =
    selectedRepo?.owner.toLowerCase() === "jetsku" &&
    selectedRepo.name.toLowerCase() === "kytyps5";
  const shaderPrecompileOption = reportedOptions.find((flag) =>
    /(?:shader.*pre.?comp|pre.?comp.*shader)/i.test(flag),
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
  const [shaderPrecompileEnabled, setShaderPrecompileEnabled] = useState(false);
  const [savingDefaults, setSavingDefaults] = useState(false);
  const [saveDefaultsError, setSaveDefaultsError] = useState("");
  const storageTaskActive = useRef(false);
  const storageTotal = storageReport?.categories.reduce(
    (total, category) => total + category.size_bytes,
    0,
  ) ?? 0;
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
  useEffect(() => {
    if (!selectedBuild || !supportsBackgroundShaderPrecompile) {
      setShaderPrecompileEnabled(false);
      return;
    }
    let mounted = true;
    void invoke<string | null>("get_setting", {
      key: `shader_precompile_enabled:${selectedBuild.id}`,
    })
      .then((value) => {
        if (!mounted) return;
        const enabled = value === null
          ? currentCompatibilityOS() === "windows"
          : value === "true";
        setShaderPrecompileEnabled(enabled);
        if (value === null) {
          void invoke("set_setting", {
            key: `shader_precompile_enabled:${selectedBuild.id}`,
            value: String(enabled),
          }).catch(() => {});
        }
      })
      .catch(() => {
        if (mounted) setShaderPrecompileEnabled(false);
      });
    return () => {
      mounted = false;
    };
  }, [selectedBuild?.id, supportsBackgroundShaderPrecompile]);
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
  const saveDefaults = async () => {
    if (savingDefaults) return;
    setSavingDefaults(true);
    setSaveDefaultsError("");
    try {
      await onSaveGlobalDefaults();
    } catch (error) {
      setSaveDefaultsError(`Settings could not be saved: ${String(error)}`);
    } finally {
      setSavingDefaults(false);
    }
  };
  return (
    <section className="page settings-page">
      <header className="settings-hero">
        <div className="settings-hero-icon"><Settings size={22} /></div>
        <div>
          <span className="eyebrow">STUDIO PREFERENCES</span>
          <h2>Settings</h2>
          <p>Configure emulator behavior, manage local files, and control Studio integrations.</p>
        </div>
        <span className="settings-version">v{installedVersion}</span>
      </header>
      <nav className="settings-nav" aria-label="Settings sections">
        <a href="#settings-emulator"><Gamepad2 size={16} /> Emulator</a>
        <a href="#settings-github"><Github size={16} /> GitHub</a>
        <a href="#settings-emulator-updates"><RefreshCw size={16} /> Emulator updates</a>
        <a href="#settings-storage"><Database size={16} /> Storage</a>
        <a href="#settings-discord"><Settings size={16} /> Discord</a>
        <a href="#settings-app-updates"><RefreshCw size={16} /> Studio updates</a>
        <a href="#settings-about"><Settings size={16} /> About</a>
      </nav>
      <div id="settings-emulator" className="panel settings-emulator-panel settings-section">
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
                return (
                  <option value={build.id} key={build.id}>
                    {emulatorBuildName(repo, build)} · {build.build_type}
                    {build.active ? " · Active" : ""}
                  </option>
                );
              })}
            </select>
          </label>
        </div>
        {selectedBuild && (
          <div className="settings-emulator-summary">
            <strong>{emulatorBuildName(selectedRepo, selectedBuild)}</strong>
            <span>
              {selectedBuild.build_type} build
              {selectedBuild.active ? " · currently active" : ""}
            </span>
          </div>
        )}
        {selectedBuild && isOfficialKytyPs5 && (
          <aside className="settings-shader-cache" role="note">
            <div className="settings-shader-cache-heading">
              <strong>Shader precompilation</strong>
              <span>{shaderPrecompileOption ?? "No precompile option reported"}</span>
            </div>
            {selectedBuild.build_type.toLowerCase() === "release" ? (
              <p>
                No complete precompile-before-launch option is available in this
                build. KytyPS5 Release builds do save Vulkan pipeline-cache data
                as the game uses shaders and can reuse it on later launches,
                but the first launch still compiles shaders as needed.
              </p>
            ) : (
              <p>
                KytyPS5 disables its persistent Vulkan pipeline cache in Debug
                builds, and does not provide a setting for full shader
                precompilation before launch.
              </p>
            )}
          </aside>
        )}
        {selectedBuild && supportsBackgroundShaderPrecompile && (
          <aside className="settings-shader-cache" role="group" aria-label="Shader precompilation">
            <div className="settings-shader-cache-heading">
              <strong>Background shader precompile</strong>
              <span>Jetsku fork</span>
            </div>
            <p>
              Records shaders seen during play and translates them in the background on later starts.
              This also enables the fork’s fast-first pipeline mode. The first run still discovers
              shaders; this does not precompile every game area.
              {hostOS === "linux" && " Linux support depends on the selected build and has not been verified here."}
            </p>
            <label className="settings-shader-precompile-toggle">
              <input
                aria-label="Enable known shader precompile on each launch"
                type="checkbox"
                checked={shaderPrecompileEnabled}
                onChange={(event) => {
                  const enabled = event.target.checked;
                  setShaderPrecompileEnabled(enabled);
                  void invoke("set_setting", {
                    key: `shader_precompile_enabled:${selectedBuild.id}`,
                    value: String(enabled),
                  }).catch(() => setShaderPrecompileEnabled(!enabled));
                }}
              />
              <span className="settings-toggle-track" aria-hidden="true"><i /></span>
              <span className="settings-toggle-label">Enable known shader precompile on each launch</span>
            </label>
          </aside>
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
              optionMetadata={globalProfileCapabilities.option_metadata}
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
              optionMetadata={globalProfileCapabilities.option_metadata}
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
            disabled={!globalProfileCapabilities || !selectedBuildId || savingDefaults}
            onClick={() => void saveDefaults()}
          >
            {savingDefaults ? "Saving…" : "Save settings"}
          </button>
        </div>
        {saveDefaultsError && <p role="alert" className="error-note">{saveDefaultsError}</p>}
      </div>
      <div className="sectionhead">
        <div>
          <h3>Settings & storage</h3>
          <p>{storageDescription}</p>
        </div>
        <Settings />
      </div>
      <div className="panel settings-section" id="settings-github">
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
      <div className="panel settings-section" id="settings-emulator-updates">
        <h3>Emulator updates</h3>
        <p>
          Emulator forks are never checked, fetched, or built automatically.
          Use Check updates on an installed build to compare it with upstream.
          If an update is available, the app asks before fetching and building
          a new verified installation. Existing builds remain installed and
          active until you choose another build.
        </p>
      </div>
      <div className="panel settings-section" id="settings-storage">
        <h3>Storage management</h3>
        <p>
          Sizes include downloaded source repositories and Studio-managed build
          data. Source repositories and original game content are not cleanup
          targets here.
        </p>
        <div className="storage-overview">
          <div className="storage-total-card">
            <span>Studio-managed data</span>
            <strong>{storageReport ? formatBytes(storageTotal) : "Not scanned"}</strong>
            <small>{storageReport ? `${storageReport.categories.length} locations` : "Refresh sizes to scan storage"}</small>
          </div>
          <div className="storage-breakdown" aria-label="Storage by category">
            {storageReport?.categories.map((category, index) => {
              const share = storageTotal > 0 ? (category.size_bytes / storageTotal) * 100 : 0;
              return (
                <div className="storage-category" key={category.name}>
                  <div className="storage-category-label">
                    <span><i className={`storage-dot tone-${index % 5}`} />{category.name}</span>
                    <strong>{formatBytes(category.size_bytes)}</strong>
                  </div>
                  <div className="storage-meter" aria-hidden="true"><span style={{ width: `${share}%` }} /></div>
                  <small className="storage-path">{category.path}</small>
                </div>
              );
            })}
          </div>
        </div>
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
      <div className="panel settings-section" id="settings-discord">
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
      <div className="panel settings-section" id="settings-app-updates">
        <h3>KytyPS5 Studio updates</h3>
        <p>
          {appUpdateStatus || "App updates are checked automatically when Studio starts."}
        </p>
        {appUpdate?.update_available && (
          <p className="hint">
            Version {appUpdate.latest_version} is available
            {appUpdate.asset_name ? ` · ${appUpdate.asset_name}` : ""}.
            {appUpdate.asset_name?.toLowerCase().endsWith(".exe")
              ? " Studio closes during installation and restarts automatically when the verified NSIS update finishes."
              : appUpdate.running_appimage
              ? " Studio installs the verified AppImage and restarts automatically to use the new version."
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
      <div className="panel settings-section" id="settings-about">
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
