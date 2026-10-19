import { useRef, useState, type Dispatch, type SetStateAction } from "react";
import { Settings } from "lucide-react";
import type { AppUpdateInfo, ProfileCapabilities, StorageReport } from "../types/studio";
import {
  ProfileOptionFields,
  cliOptionInfo,
} from "../components/settings/ProfileOptionFields";
import { formatBytes } from "../utils/format";

export type SettingsPageProps = {
  globalProfileCapabilities: ProfileCapabilities | null;
  globalProfileOptions: Record<string, string>;
  setGlobalProfileOptions: Dispatch<SetStateAction<Record<string, string>>>;
  globalUnsupported: boolean;
  gpuDevices: { index: number; name: string }[];
  githubTokenAvailable: boolean;
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
};

export function SettingsPage({
  globalProfileCapabilities,
  globalProfileOptions,
  setGlobalProfileOptions,
  globalUnsupported,
  gpuDevices,
  githubTokenAvailable,
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
}: SettingsPageProps) {
  const [storageTask, setStorageTask] = useState<string | null>(null);
  const storageTaskActive = useRef(false);
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
      <div className="panel">
        <h3>Global KytyPS5 defaults</h3>
        <p>
          These supported options apply to games that do not override them.
          Leave an option out to use the emulator’s own default. Availability
          follows the active emulator build.
        </p>
        <small className="hint">
          Supported by active build:{" "}
          {globalProfileCapabilities?.options
            .map((flag) => cliOptionInfo[flag]?.label || flag)
            .join(" · ") || "No active build or no additional options detected"}
        </small>
        {globalUnsupported && (
          <div className="diagnosis-note">
            Some saved defaults are not supported by the active build. They will
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
        <ProfileOptionFields
          available={globalProfileCapabilities?.options || []}
          options={globalProfileOptions}
          switchOptions={globalProfileCapabilities?.switch_options || []}
          previewTitle="Global default arguments"
          onChange={setGlobalProfileOptions}
          gpuDevices={gpuDevices}
        />
        <button
          className="primary"
          disabled={!globalProfileCapabilities}
          onClick={onSaveGlobalDefaults}
        >
          Save global defaults
        </button>
      </div>
      <div className="sectionhead">
        <div>
          <h3>Settings & storage</h3>
          <p>Application-managed files follow XDG user directories.</p>
        </div>
        <Settings />
      </div>
      <div className="panel">
        <h3>GitHub access</h3>
        <p>
          Anonymous access works within GitHub rate limits. A personal access
          token is stored in Linux Secret Service, never in Studio's database.
        </p>
        <div className="kv">
          <span>Credential status</span>
          <strong>
            {githubTokenAvailable
              ? "Secure token available"
              : "Anonymous API access"}
          </strong>
        </div>
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
          Sizes are measured from Studio-managed folders. Source repositories
          and original game content are not cleanup targets.
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
            {appUpdate.running_appimage
              ? " The verified AppImage will replace this one after download. Restart Studio to use it."
              : " In-place installation is available when Studio runs from an AppImage."}
          </p>
        )}
        <button className="outline" disabled={appUpdateBusy} onClick={() => onCheckAppUpdate?.()}>
          {appUpdateBusy ? "Checking / updating…" : "Check for App Update"}
        </button>
      </div>
      <div className="panel">
        <h3>About</h3>
        <p>KytyPS5 Studio 0.1.0 · Linux foundation</p>
        <p>
          Emulator source and games are supplied separately by the user. KytyPS5
          is GPL-2.0 licensed. Studio does not include firmware or games.
        </p>
      </div>
    </section>
  );
}
