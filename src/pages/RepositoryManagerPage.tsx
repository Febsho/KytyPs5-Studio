import type { Dispatch, SetStateAction } from "react";
import { useEffect, useRef, useState } from "react";
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
  onUpdateNow: (id: number) => void;
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
  const [updateStatus, setUpdateStatus] = useState<{
    repositoryId: number;
    message: string;
  } | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const updateCheckRequest = useRef(0);
  const trackedRepos = repos.filter((repo) => !repo.removed);
  const visibleRepos = trackedRepos.filter(
    (repo) => !(repo.owner.toLowerCase() === "kytyps5" && repo.name.toLowerCase() === "kytyps5"),
  );
  const selectedRepository = currentRepo?.removed ? undefined : currentRepo;
  const officialRepository = trackedRepos.find(
    (repo) => repo.owner.toLowerCase() === "kytyps5" && repo.name.toLowerCase() === "kytyps5",
  );
  useEffect(() => {
    updateCheckRequest.current += 1;
    setUpdateStatus(null);
    setCheckingUpdates(false);
  }, [selectedRepository?.id]);

  const checkSelectedRepositoryUpdates = async () => {
    if (!selectedRepository) return;
    const repositoryId = selectedRepository.id;
    const request = ++updateCheckRequest.current;
    setCheckingUpdates(true);
    setUpdateStatus(null);
    try {
      const message = await onCheckRepositoryUpdates(repositoryId);
      if (request === updateCheckRequest.current) {
        setUpdateStatus({ repositoryId, message });
      }
    } catch (error) {
      if (request === updateCheckRequest.current) {
        setUpdateStatus({
          repositoryId,
          message: `Check failed: ${String(error)}`,
        });
      }
    } finally {
      if (request === updateCheckRequest.current) setCheckingUpdates(false);
    }
  };

  const selectedUpdateStatus =
    updateStatus &&
    selectedRepository &&
    updateStatus.repositoryId === selectedRepository.id
      ? updateStatus.message
      : null;
  const updateAvailable = selectedUpdateStatus?.startsWith("Update available:") ?? false;
  const updateStatusTitle = !selectedUpdateStatus
    ? ""
    : updateAvailable
      ? "A newer build is available"
      : selectedUpdateStatus.startsWith("Check failed:")
        ? "Update check failed"
        : selectedUpdateStatus.startsWith("Installed build is current")
          ? "Installed build is current"
          : "Your build is up to date";
  const updateComparison = updateAvailable
    ? selectedUpdateStatus!.slice("Update available:".length).split(";")[0].trim()
    : "";
  const [installedCommit, availableCommit] = updateComparison
    .split("→")
    .map((commit) => commit.trim());
  const updateNote = updateAvailable
    ? selectedUpdateStatus!.slice("Update available:".length).split(";").slice(1).join(";").trim()
    : selectedUpdateStatus?.startsWith("Check failed:")
      ? selectedUpdateStatus.slice("Check failed:".length).trim()
      : selectedUpdateStatus?.startsWith("Installed build is current;")
        ? selectedUpdateStatus.slice("Installed build is current;".length).trim()
        : "";
  return (
    <section className="page">
      <div className="sectionhead">
        <div>
          <h3>Repositories</h3>
          <p>Manage KytyPS5 forks and source checkouts. Build Studio handles submodules when building.</p>
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
          <p>The upstream open-source PlayStation 5 translation layer.</p>
          <button className="outline" onClick={onAddOfficialRepository}>
            Select Official Source
          </button>
        </article>
        {visibleRepos.map((r) => (
          <article
            className={"card " + (selectedRepo === r.id ? "selected" : "")}
            key={r.id}
            onClick={() => onSelectRepository(r.id)}
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
              disabled={checkingUpdates || busy || !selectedRepository.source_path}
              onClick={() => void checkSelectedRepositoryUpdates()}
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
          {checkingUpdates && (
            <div className="repo-update-status checking" role="status">
              <div>
                <strong>Checking for updates…</strong>
                <small>Fetching the selected repository’s tracked branch from GitHub.</small>
              </div>
            </div>
          )}
          {selectedUpdateStatus && (
            <div
              className={`repo-update-status ${updateAvailable ? "available" : selectedUpdateStatus.startsWith("Check failed:") ? "failed" : "current"}`}
              role="status"
              aria-live="polite"
            >
              <div>
                <strong>{updateStatusTitle}</strong>
                {updateAvailable ? (
                  <div className="repo-update-commits">
                    <span>Installed <code>{installedCommit || "Unknown"}</code></span>
                    <span>Available <code>{availableCommit || "Unknown"}</code></span>
                  </div>
                ) : null}
                {updateNote ? <small>{updateNote}</small> : null}
                {updateAvailable && !["official", "kyty-cmake"].includes(selectedRepository.adapter) ? (
                  <small>Building this repository is unavailable until its KytyPS5 CMake adapter is verified.</small>
                ) : null}
              </div>
              {updateAvailable ? (
                <button
                  className="primary"
                  disabled={busy || !["official", "kyty-cmake"].includes(selectedRepository.adapter)}
                  onClick={() => onUpdateNow(selectedRepository.id)}
                >
                  Fetch latest and build
                </button>
              ) : null}
            </div>
          )}
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
