import type { Dispatch, SetStateAction } from "react";
import { Settings } from "lucide-react";
import type { ProfileCapabilities, StorageReport } from "../types/studio";
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
  autoUpdateMode: "notify" | "build";
  storageReport: StorageReport | null;
  busy: boolean;
  activeSessions: boolean;
  onSaveGlobalDefaults: () => void;
  onSaveGithubToken: (token: string) => void;
  onDeleteGithubToken: () => void;
  onUpdateIntervalChange: (value: string) => void;
  onAutoUpdateModeChange: (value: "notify" | "build") => void;
  onRefreshStorage: () => void;
  onCleanIncompleteBuilds: () => void;
  onClearCompilerCache: () => void;
  onClearCommunityWorktrees: () => void;
  onClearSessionLogs: () => void;
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
  autoUpdateMode,
  storageReport,
  busy,
  activeSessions,
  onSaveGlobalDefaults,
  onSaveGithubToken,
  onDeleteGithubToken,
  onUpdateIntervalChange,
  onAutoUpdateModeChange,
  onRefreshStorage,
  onCleanIncompleteBuilds,
  onClearCompilerCache,
  onClearCommunityWorktrees,
  onClearSessionLogs,
}: SettingsPageProps) {
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
          <button className="outline" onClick={onRefreshStorage}>
            Refresh sizes
          </button>
          <button
            className="outline danger"
            disabled={
              busy ||
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
                onCleanIncompleteBuilds();
            }}
          >
            Clear incomplete builds
          </button>
          <button
            className="outline danger"
            disabled={
              busy ||
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
                  `Remove ${formatBytes(bytes)} of rebuildable compiler cache? Installed emulator builds, repositories, games, profiles and logs will be kept.`,
                )
              )
                onClearCompilerCache();
            }}
          >
            Clear compiler cache
          </button>
          <button
            className="outline danger"
            disabled={
              busy || activeSessions ||
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
                onClearCommunityWorktrees();
            }}
          >
            Clear temporary community files
          </button>
          <button
            className="outline danger"
            disabled={
              busy || activeSessions ||
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
                onClearSessionLogs();
            }}
          >
            Clear session logs
          </button>
        </div>
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
