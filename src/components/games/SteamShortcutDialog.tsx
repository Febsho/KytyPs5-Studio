import { useEffect, useMemo, useState } from "react";
import { X } from "lucide-react";
import type { Build, Game, Repo } from "../../types/studio";
import { loadGameIcon, retainGameIcon } from "../../services/gameArtwork";
import { short } from "../../utils/format";

type Props = {
  game: Game;
  builds: Build[];
  repos: Repo[];
  selectedBuildId: number | null;
  busy: boolean;
  onSelectBuild: (id: number) => void;
  onAdd: () => void;
  onClose: () => void;
};

export function SteamShortcutDialog({ game, builds, repos, selectedBuildId, busy, onSelectBuild, onAdd, onClose }: Props) {
  const [icon, setIcon] = useState("");
  useEffect(() => {
    let current = true;
    void loadGameIcon(game.id).then((url) => { if (current) setIcon(url || ""); }).catch(() => { if (current) setIcon(""); });
    return () => { current = false; retainGameIcon(game.id); };
  }, [game.id]);
  const selectedBuild = builds.find((build) => build.id === selectedBuildId) ?? null;
  const availableBuilds = useMemo(() => builds.filter((build) => build.executable_status === "Executable present"), [builds]);
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
      <section className="steam-shortcut-dialog" role="dialog" aria-modal="true" aria-labelledby="steam-shortcut-title">
        <header className="steam-shortcut-header">
          <div><h2 id="steam-shortcut-title">Add to Steam</h2><p>Choose which fork and installed build Steam should launch.</p></div>
          <button className="icon-button" aria-label="Close" disabled={busy} onClick={onClose}><X size={18} /></button>
        </header>
        <div className="steam-shortcut-preview">
          {icon ? <img src={icon} alt={`${game.title} artwork`} /> : <div className="steam-art-placeholder">{game.title.slice(0, 1)}</div>}
          <div><strong>{game.title}</strong><small>Steam library name: {game.title} · KytyPS5 Studio</small><small>Artwork: game icon from sce_sys/icon0.png</small></div>
        </div>
        <label className="steam-build-select">Fork / emulator build
          <select value={selectedBuildId ?? ""} onChange={(event) => onSelectBuild(Number(event.target.value))} disabled={busy || !availableBuilds.length}>
            {availableBuilds.map((build) => {
              const repo = repos.find((item) => item.id === build.repo_id);
              return <option key={build.id} value={build.id}>{repo ? `${repo.owner}/${repo.name}` : "Unknown fork"} · {build.build_type} · {short(build.commit_sha)}{build.active ? " · active" : ""}</option>;
            })}
          </select>
        </label>
        {selectedBuild && <div className="steam-launch-metadata"><span><b>Executable</b>{selectedBuild.path}</span><span><b>Game</b>{game.path}</span><span><b>Launch</b>Emulator with --game and this game's saved profile options</span></div>}
        {!availableBuilds.length && <p className="error">No installed executable builds are available. Build an emulator fork first.</p>}
        <footer><button className="outline" disabled={busy} onClick={onClose}>Cancel</button><button disabled={busy || !selectedBuild || !availableBuilds.some((build) => build.id === selectedBuildId)} onClick={onAdd}>{busy ? "Adding…" : "Add shortcut to Steam"}</button></footer>
      </section>
    </div>
  );
}
