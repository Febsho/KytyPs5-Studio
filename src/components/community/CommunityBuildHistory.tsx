import { GitPullRequest } from "lucide-react";
import type {
  CommunityBuildHistory as BuildHistoryEntry,
  CommunityPRPage,
} from "../../types/studio";
import { date, short } from "../../utils/format";

export function CommunityBuildHistory({
  history,
  pullRequests,
  busy,
  reviewLoading,
  onActivate,
  onReview,
  onRename,
}: {
  history: BuildHistoryEntry[];
  pullRequests: CommunityPRPage | null;
  busy: boolean;
  reviewLoading: boolean;
  onActivate: (buildId: number) => void;
  onReview: (entry: BuildHistoryEntry) => void;
  onRename: (entry: BuildHistoryEntry) => void;
}) {
  return (
    <>
      <div className="sectionhead">
        <div>
          <h3>Community build history</h3>
          <p>
            Experimental builds remain separate from existing installations.
            Activation uses the standard emulator manager.
          </p>
        </div>
      </div>
      {history.length ? (
        <div className="build-list">
          {history.map((entry) => {
            const hasNewHeads =
              pullRequests?.repository === entry.repository &&
              entry.pull_requests.some(([number, sha]) =>
                pullRequests.items.some(
                  (pr) => pr.number === number && pr.head_sha !== sha,
                ),
              );
            return (
              <div className="build-item" key={entry.result_id}>
                <div className="build-icon">
                  <GitPullRequest />
                </div>
                <div>
                  <strong>
                    {entry.name} <span className="subtle">{entry.status}</span>
                  </strong>
                  <small>
                    {entry.repository} · base {short(entry.base_sha)} (
                    {entry.base_branch})<br />
                    {entry.pull_requests
                      .map(([number, sha]) => `#${number} ${short(sha)}`)
                      .join(" · ")}
                    <br />
                    {hasNewHeads
                      ? "PR commits updated on GitHub · rebuild available"
                      : ""}
                    <br />
                    {date(entry.started_at)}
                    {entry.error ? ` · ${entry.error}` : ""}
                    <br />
                    {entry.install_path || "No installation created"}
                  </small>
                </div>
                <span className="badge">
                  {entry.build_type} · {entry.compiler} · {entry.jobs} jobs
                  {entry.clean ? " · clean" : ""}
                </span>
                {entry.active ? (
                  <span className="badge success">Active</span>
                ) : (
                  entry.build_id !== null && (
                    <button
                      className="outline"
                      onClick={() => onActivate(entry.build_id!)}
                    >
                      Activate
                    </button>
                  )
                )}
                {entry.status === "verified" && (
                  <button
                    className="outline"
                    disabled={busy || reviewLoading}
                    onClick={() => onReview(entry)}
                  >
                    Rebuild / duplicate
                  </button>
                )}
                <button className="outline" onClick={() => onRename(entry)}>
                  Rename
                </button>
              </div>
            );
          })}
        </div>
      ) : (
        <div className="empty compact">
          <GitPullRequest />
          <strong>No community builds yet</strong>
        </div>
      )}
    </>
  );
}
