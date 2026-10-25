import type { Dispatch, SetStateAction } from "react";
import type {
  Game,
  Profile,
  ProfileCapabilities,
} from "../../types/studio";
import { availableProfileOptions, profileWithOptions } from "../../utils/profile";
import { ProfileActions } from "./ProfileActions";
import { ProfileOptionFields } from "./ProfileOptionFields";

type Props = {
  gameId: number;
  games: Game[];
  profile: Profile;
  setProfile: Dispatch<SetStateAction<Profile>>;
  capabilities: ProfileCapabilities | null;
  globalOptions: Record<string, string>;
  gpuDevices: { index: number; name: string }[];
  sourceGameId: number | null;
  setSourceGameId: (id: number | null) => void;
  loading: boolean;
  dirty: boolean;
  canRestore: boolean;
  unsupported: boolean;
  onCopy: (gameId: number) => void;
  onRestore: () => void;
  onClearUnsupported: () => void;
  onSave: () => void;
  onReset: () => void;
};

export function GameProfileEditor({
  gameId,
  games,
  profile,
  setProfile,
  capabilities,
  globalOptions,
  gpuDevices,
  sourceGameId,
  setSourceGameId,
  loading,
  dirty,
  canRestore,
  unsupported,
  onCopy,
  onRestore,
  onClearUnsupported,
  onSave,
  onReset,
}: Props) {
  return (
    <div className="profile">
      <div className="game-profile-heading">
        <div>
          <h3>Game profile</h3>
          <p>Override the emulator defaults for this game. Clear a field to inherit the selected build’s global value.</p>
        </div>
        {capabilities && <span>{availableProfileOptions(capabilities).length} options available in this build</span>}
      </div>
      <ProfileActions
        games={games}
        currentGameId={gameId}
        sourceGameId={sourceGameId}
        loading={loading}
        dirty={dirty}
        canRestore={canRestore}
        onSourceChange={setSourceGameId}
        onCopy={onCopy}
        onRestore={onRestore}
      />
      <ProfileOptionFields
        available={availableProfileOptions(capabilities)}
        optionMetadata={capabilities?.option_metadata}
        options={profile.options}
        inheritedOptions={globalOptions}
        switchOptions={capabilities?.switch_options || []}
        previewTitle="Effective launch arguments"
        onChange={(options) =>
          setProfile((current) => profileWithOptions(current, options))
        }
        gpuDevices={gpuDevices}
        showUnavailable={false}
      />
      {availableProfileOptions(capabilities).length === 0 && capabilities && (
        <div className="settings-empty-options">
          <strong>This build advertises no configurable launch settings</strong>
          <span>Studio reads each build’s installed executable directly, so the displayed options follow that build rather than the newest upstream release.</span>
        </div>
      )}
      {unsupported && (
        <button className="outline" onClick={onClearUnsupported}>
          Clear unsupported saved options
        </button>
      )}
      <p className="hint">
        Options are read from this build’s own executable help. Empty values inherit this build’s global defaults; source-unverified options are marked in their capability details.
      </p>
      <div className="buttonrow">
        <button
          className="outline"
          disabled={!capabilities || loading}
          onClick={onSave}
        >
          {loading ? "Loading profile…" : "Save profile"}
        </button>
        <button
          className="outline"
          disabled={!capabilities || loading}
          onClick={onReset}
        >
          Reset game profile
        </button>
      </div>
    </div>
  );
}
