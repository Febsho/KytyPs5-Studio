import type { CommunityPRDetails } from "../../types/studio";
import { short } from "../../utils/format";

export function CommunityPullRequestDrawer({
  details,
  onClose,
}: {
  details: CommunityPRDetails | null;
  onClose: () => void;
}) {
  if (!details) return null;
  const pr = details.pull_request;

  return (
    <aside
      className="pr-drawer"
      aria-label={`Pull request ${pr.number} details`}
    >
      <button className="drawer-close outline" onClick={onClose}>
        Close
      </button>
      <h3>
        <a href={pr.html_url} target="_blank" rel="noreferrer">
          #{pr.number} {pr.title}
        </a>
      </h3>
      <p>
        {pr.author} · {pr.head_branch} → {pr.base_branch}
      </p>
      <p className="pr-description">{pr.body || "No description provided."}</p>
      <div className="kv">
        <span>Mergeability</span>
        <strong>
          {pr.mergeable === null
            ? "Unknown or still calculating"
            : pr.mergeable
              ? "Mergeable"
              : "Conflicting"}{" "}
          · {pr.mergeable_state || "state unavailable"}
        </strong>
      </div>
      <div className="kv">
        <span>CI status</span>
        <strong>{details.checks_state}</strong>
      </div>
      {details.checks.length > 0 && (
        <div className="pr-checks">
          <h4>Associated checks</h4>
          {details.checks.map((check) => (
            <a
              key={`${check.name}-${check.html_url}`}
              href={check.html_url}
              target="_blank"
              rel="noreferrer"
            >
              <span>{check.name}</span>
              <small>
                {check.status}
                {check.conclusion ? ` · ${check.conclusion}` : ""}
              </small>
            </a>
          ))}
        </div>
      )}
      <h4>Commit history</h4>
      {details.commits.map((commit) => (
        <p key={commit.sha} className="pr-commit">
          <code>{short(commit.sha)}</code> {commit.commit.message}
        </p>
      ))}
      <h4>Changed files</h4>
      {details.files.map((file) => (
        <details key={file.filename}>
          <summary>
            {file.filename} · +{file.additions} / −{file.deletions}
          </summary>
          <pre>{file.patch || "Patch unavailable from GitHub."}</pre>
        </details>
      ))}
      {details.subresource_errors.map((error) => (
        <p className="diagnosis-note" key={error}>
          {error}
        </p>
      ))}
    </aside>
  );
}
