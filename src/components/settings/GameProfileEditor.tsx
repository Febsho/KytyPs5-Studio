import type { Dispatch, SetStateAction } from "react";
import type {
  Game,
  Profile,
  ProfileCapabilities,
} from "../../types/studio";
import { profileWithOptions } from "../../utils/profile";
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
      <h3>Game profile</h3>
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
        available={capabilities?.options || []}
        options={profile.options}
        inheritedOptions={globalOptions}
        switchOptions={capabilities?.switch_options || []}
        previewTitle="Effective launch arguments"
        onChange={(options) =>
          setProfile((current) => profileWithOptions(current, options))
        }
        gpuDevices={gpuDevices}
      />
      {unsupported && (
        <button className="outline" onClick={onClearUnsupported}>
          Clear unsupported saved options
        </button>
      )}
      <p className="hint">
        Only options reported by this selected emulator build are enabled. Empty
        values inherit global defaults or KytyPS5 defaults.
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
