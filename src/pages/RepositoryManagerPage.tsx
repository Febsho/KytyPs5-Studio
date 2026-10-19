import type { Dispatch, SetStateAction } from "react";
import { useState } from "react";
import { ArrowDownToLine, ChevronRight, GitBranch, Search, Trash2 } from "lucide-react";
import type { Fork, GithubDetails, Repo } from "../types/studio";
import { short } from "../utils/format";

export type RepositoryManagerPageProps = {
  forks: Fork[];
  repos: Repo[];
  currentRepo?: Repo;
  selectedRepo: number | null;
  repoUrl: string;
  setRepoUrl: Dispatch<SetStateAction<string>>;
  githubDetails: GithubDetails | null;
  busy: boolean;
  onDiscoverForks: () => void;
  onAddManualRepository: () => void;
  onAddOfficialRepository: () => void;
  onSelectRepository: (id: number) => void;
  onCloneRepository: (id: number) => void;
  onOpenSourceFolder: (id: number) => void;
  onInspectRepository: (id: number) => void;
  onCheckRepositoryUpdates: (id: number) => Promise<string>;
  onRefreshGitHubDetails: (id: number) => void;
  onOpenBuildStudio: () => void;
  onAddFork: (fork: Fork) => void;
  onRemoveRepository?: (id:number) => void;
  onUpdateNow?: (id:number) => void;
};

export function RepositoryManagerPage({
  forks,
  repos,
  currentRepo,
  selectedRepo,
  repoUrl,
  setRepoUrl,
  githubDetails,
  busy,
  onDiscoverForks,
  onAddManualRepository,
  onAddOfficialRepository,
  onSelectRepository,
  onCloneRepository,
  onOpenSourceFolder,
  onInspectRepository,
  onCheckRepositoryUpdates,
  onRefreshGitHubDetails,
  onOpenBuildStudio,
  onAddFork,
  onRemoveRepository,
  onUpdateNow,
}: RepositoryManagerPageProps) {
  const [updateStatus, setUpdateStatus] = useState("");
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const trackedRepos = repos.filter((repo) => !repo.removed);
  const visibleRepos = trackedRepos.filter(
    (repo) => !(repo.owner.toLowerCase() === "kytyps5" && repo.name.toLowerCase() === "kytyps5"),
  );
  const selectedRepository = currentRepo?.removed ? undefined : currentRepo;
  const officialRepository = trackedRepos.find(
    (repo) => repo.owner.toLowerCase() === "kytyps5" && repo.name.toLowerCase() === "kytyps5",
  );
  return (
    <section className="page">
      <div className="sectionhead">
        <div>
          <h3>Repositories</h3>
          <p>Manage independent KytyPS5 source checkouts.</p>
        </div>
        <button className="outline" onClick={onDiscoverForks}>
          <Search size={17} /> Discover forks
        </button>
      </div>
      <div className="add-row">
        <input
          value={repoUrl}
          onChange={(e) => setRepoUrl(e.target.value)}
          placeholder="https://github.com/owner/repository"
        />
        <button className="primary" onClick={onAddManualRepository}>
          Add repository
        </button>
      </div>
      <div className="repo-grid">
        <article className={`card accent ${officialRepository && officialRepository.id === selectedRepository?.id ? "selected" : ""}`}>
          <div className="cardtop">
            <GitBranch />
            <span>OFFICIAL</span>
          </div>
          <h3>KytyPS5 / KytyPS5</h3>
          <p>The upstream PlayStation 5 translation layer for Linux.</p>
          <button className="outline" onClick={onAddOfficialRepository}>
            Select Official Source
          </button>
        </article>
        {visibleRepos.map((r) => (
          <article
            className={"card " + (selectedRepo === r.id ? "selected" : "")}
            key={r.id}
            onClick={() => { setUpdateStatus(""); onSelectRepository(r.id); }}
          >
            <div className="cardtop">
              <GitBranch />
              <span>
                {["official", "kyty-cmake"].includes(r.adapter)
                  ? "ADAPTER MATCH"
                  : r.source_path
                    ? "UNVERIFIED"
                    : "NOT CLONED"}
              </span>
            </div>
            <h3>
              {r.owner} / {r.name}
            </h3>
            <p>{r.url}</p>
            <div className="cardfoot">
              <span>{short(r.installed_commit)}</span>
              <span>{r.branch || "Default branch pending clone"}</span>
            </div>
            <button className="repo-remove-button outline" title={`Remove ${r.owner}/${r.name}`} aria-label={`Remove ${r.owner}/${r.name}`} onClick={(event) => { event.stopPropagation(); onRemoveRepository?.(r.id); }}><Trash2 size={14} /> Remove</button>
          </article>
        ))}
      </div>
      {selectedRepository && (
        <div className="panel">
          <div className="sectionhead">
            <div>
              <h3>
                {selectedRepository.owner} / {selectedRepository.name}
              </h3>
              <p>
                {selectedRepository.source_path || "Source checkout not installed"}
              </p>
            </div>
            <span className="badge">{selectedRepository.adapter}</span>
          </div>
          <div className="buttonrow">
            <button
              className="primary"
              disabled={busy}
              onClick={() => onCloneRepository(selectedRepository.id)}
            >
              <ArrowDownToLine size={16} /> Clone / restore checkout
            </button>
            <button
              className="outline"
              disabled={!selectedRepository.source_path}
              onClick={() => onOpenSourceFolder(selectedRepository.id)}
            >
              Open source folder
            </button>
            <button
              className="outline"
              disabled={!selectedRepository.source_path}
              onClick={() => onInspectRepository(selectedRepository.id)}
            >
              Inspect adapter
            </button>
            <button
              className="outline"
              disabled={checkingUpdates || !selectedRepository.source_path}
              onClick={async () => {
                setCheckingUpdates(true); setUpdateStatus("");
                try { setUpdateStatus(await onCheckRepositoryUpdates(selectedRepository.id)); }
                catch (error) { setUpdateStatus(`Check failed: ${String(error)}`); }
                finally { setCheckingUpdates(false); }
              }}
            >
              {checkingUpdates ? "Checking…" : "Check updates"}
            </button>
            <button
              className="outline"
              onClick={() => onRefreshGitHubDetails(selectedRepository.id)}
            >
              Refresh GitHub details
            </button>
            <button className="outline" onClick={onOpenBuildStudio}>
              Build <ChevronRight size={15} />
            </button>
          </div>
          {updateStatus && <div className={`repo-update-status ${updateStatus.startsWith("Update available:") ? "available" : ""}`}>
            <div><strong>{updateStatus.startsWith("Update available:") ? "Update available" : updateStatus.startsWith("Check failed:") ? "Update check failed" : updateStatus}</strong><small>{updateStatus.startsWith("Update available:") ? updateStatus.slice("Update available:".length).trim() : ""}</small></div>
            {updateStatus.startsWith("Update available:") && <button className="primary" disabled={busy} onClick={()=>onUpdateNow?.(selectedRepository.id)}>Update Now</button>}
          </div>}
          {githubDetails && (
            <div className="github-details">
              <div className="game-meta">
                <span>{githubDetails.fork ? "Fork" : "Repository"}</span>
                <span>★ {githubDetails.stars}</span>
                <span>Default: {githubDetails.default_branch}</span>
                <span>
                  Tracked: {selectedRepository.branch || githubDetails.default_branch}
                </span>
                <span>
                  Updated{" "}
                  {new Date(githubDetails.updated_at).toLocaleDateString()}
                </span>
              </div>
              <div className="kv">
                <span>Latest tracked-branch commit</span>
                <strong>
                  {githubDetails.latest_commit
                    ? short(githubDetails.latest_commit.sha)
                    : "No commit found"}
                </strong>
              </div>
              {githubDetails.latest_commit && (
                <>
                  <p>
                    {githubDetails.latest_commit.commit.message.split("\n")[0]}
                  </p>
                  <small>
                    {githubDetails.latest_commit.commit.author?.date
                      ? new Date(
                          githubDetails.latest_commit.commit.author.date,
                        ).toLocaleString()
                      : "Commit date unavailable"}{" "}
                    · installed {short(selectedRepository.installed_commit)}
                  </small>
                </>
              )}
              <h4>Releases</h4>
              {githubDetails.releases.length ? (
                githubDetails.releases.map((release) => (
                  <div className="kv" key={release.tag_name}>
                    <span>
                      {release.name || release.tag_name}
                      {release.prerelease ? " · prerelease" : ""}
                    </span>
                    <strong>
                      {release.published_at
                        ? new Date(release.published_at).toLocaleDateString()
                        : "Date unavailable"}
                    </strong>
                  </div>
                ))
              ) : (
                <p>No published releases were found.</p>
              )}
            </div>
          )}
        </div>
      )}
      {forks.length > 0 && (
        <>
          <div className="sectionhead">
            <h3>Recently updated forks</h3>
            <small>GitHub API · select to track</small>
          </div>
          <div className="fork-list">
            {forks.map((f) => (
              <div key={f.full_name}>
                <div>
                  <strong>{f.full_name}</strong>
                  <small>
                    {f.description || "No description"} · ★ {f.stargazers_count}{" "}
                    · {f.license?.spdx_id || "License unknown"} ·{" "}
                    {f.fork ? "Fork" : "Repository"} · default branch{" "}
                    {f.default_branch} · updated{" "}
                    {new Date(f.updated_at).toLocaleDateString()}
                  </small>
                  <span className="badge">
                    {trackedRepos.some((r) => r.url === f.html_url)
                      ? "Already tracked"
                      : "Build compatibility unknown until source inspection"}
                  </span>
                </div>
                <button onClick={() => onAddFork(f)}>Add</button>
              </div>
            ))}
          </div>
        </>
      )}
    </section>
  );
}
