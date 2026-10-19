import { Play } from "lucide-react";
import type { CommunityPR, CommunityPRDetails } from "../../types/studio";
import { short } from "../../utils/format";

function reviewHasMissingFiles(review: CommunityPRDetails[]) {
  return review.some(
    (detail) =>
      detail.subresource_errors.some((error) =>
        error.startsWith("Changed files unavailable"),
      ) ||
      (detail.files.length === 0 && detail.pull_request.changed_files !== 0) ||
      (detail.pull_request.changed_files !== null &&
        detail.pull_request.changed_files > detail.files.length),
  );
}

export function CommunityPRSelection({
  selectedPullRequests,
  review,
  targetRepository,
  targetBranch,
  pinnedHeads,
  busy,
  reviewLoading,
  buildType,
  jobs,
  cleanBuild,
  onMove,
  onClear,
  onReview,
  onBuild,
  onBuildTypeChange,
  onJobsChange,
  onCleanChange,
}: {
  selectedPullRequests: CommunityPR[];
  review: CommunityPRDetails[];
  targetRepository?: string;
  targetBranch?: string;
  pinnedHeads: Record<number, string>;
  busy: boolean;
  reviewLoading: boolean;
  buildType: string;
  jobs: number;
  cleanBuild: boolean;
  onMove: (index: number, direction: -1 | 1) => void;
  onClear: () => void;
  onReview: () => void;
  onBuild: (review: CommunityPRDetails[]) => void;
  onBuildTypeChange: (type: string) => void;
  onJobsChange: (jobs: number) => void;
  onCleanChange: (clean: boolean) => void;
}) {
  if (!selectedPullRequests.length) return null;
  const hasDifferentReviewBases =
    review.length === selectedPullRequests.length &&
    new Set(review.map((detail) => detail.pull_request.base_sha)).size > 1;

  return (
    <>
      <div className="community-selection">
        <div>
          <strong>
            {selectedPullRequests.length} PR
            {selectedPullRequests.length === 1 ? "" : "s"} selected
          </strong>
          <small>
            Add to {targetRepository || "Selected repository"} · branch {targetBranch || "default branch"}. Apply order:{" "}
            {selectedPullRequests.map((pr) => `#${pr.number}`).join(" → ")}. Git
            worktrees isolate source files, not untrusted build code.
          </small>
          <div className="pr-order">
            {selectedPullRequests.map((pr, index) => (
              <span key={pr.number}>
                #{pr.number}
                <button
                  aria-label={`Move PR ${pr.number} earlier`}
                  disabled={index === 0}
                  onClick={() => onMove(index, -1)}
                >
                  ↑
                </button>
                <button
                  aria-label={`Move PR ${pr.number} later`}
                  disabled={index === selectedPullRequests.length - 1}
                  onClick={() => onMove(index, 1)}
                >
                  ↓
                </button>
              </span>
            ))}
          </div>
        </div>
        <button className="outline" onClick={onClear}>
          Clear
        </button>
        <button
          className="outline"
          disabled={busy || reviewLoading}
          onClick={onReview}
        >
          {reviewLoading ? "Reviewing PR files…" : "Review selection"}
        </button>
      </div>
      {review.length === selectedPullRequests.length && (
        <div className="panel pr-review">
          <h3>Review before building</h3>
          <p>
            Confirm the exact selected head SHAs and inspect build scripts and
            submodule changes. Compilation executes community source; the
            isolated clone is not a security sandbox.
          </p>
          {review.map((detail) => (
            <div className="pr-review-item" key={detail.pull_request.number}>
              <strong>
                #{detail.pull_request.number} {detail.pull_request.title}
              </strong>
              <small>
                Head {short(detail.pull_request.head_sha)}
                {pinnedHeads[detail.pull_request.number] &&
                pinnedHeads[detail.pull_request.number] !==
                  detail.pull_request.head_sha
                  ? ` · updated from ${short(pinnedHeads[detail.pull_request.number])}`
                  : ""}{" "}
                · base {short(detail.pull_request.base_sha)} · mergeability{" "}
                {detail.pull_request.mergeable === null
                  ? "unknown"
                  : detail.pull_request.mergeable
                    ? "reported mergeable"
                    : "reported conflicting"}
              </small>
              <ul>
                {detail.files.map((file) => {
                  const sensitive =
                    /\.gitmodules|CMakeLists|(^|\/)cmake\/|(^|\/)(Makefile|meson\.build|build\.ninja)|\.(sh|cmake)$/i.test(
                      file.filename,
                    );
                  return (
                    <li key={file.filename}>
                      <code>{file.filename}</code>
                      {sensitive && (
                        <span className="badge warning">
                          Build / submodule configuration
                        </span>
                      )}
                      <small>
                        {file.status} · +{file.additions} / −{file.deletions}
                      </small>
                    </li>
                  );
                })}
              </ul>
              {detail.pull_request.changed_files !== null &&
                detail.pull_request.changed_files > detail.files.length && (
                  <p className="diagnosis-note">
                    GitHub reported {detail.pull_request.changed_files} changed
                    files; {detail.files.length} are shown by the current API
                    page.
                  </p>
                )}
              {detail.subresource_errors
                .filter((error) =>
                  error.startsWith("Changed files unavailable"),
                )
                .map((error) => (
                  <p className="diagnosis-note" key={error}>
                    {error}
                  </p>
                ))}
            </div>
          ))}
          {hasDifferentReviewBases && (
            <p className="diagnosis-note">
              These PRs were reviewed against different commits. Studio will apply them in the selected order to the latest commit on their shared target branch; Git conflicts will stop the build.
            </p>
          )}
          <div className="formgrid pr-build-options">
            <div>
              <label>Build type</label>
              <select
                value={buildType}
                onChange={(event) => onBuildTypeChange(event.target.value)}
              >
                <option>Release</option>
                <option>Debug</option>
                <option>RelWithDebInfo</option>
              </select>
            </div>
            <div>
              <label>Parallel jobs</label>
              <input
                type="number"
                min="1"
                max="128"
                value={jobs}
                onChange={(event) =>
                  onJobsChange(
                    Math.max(1, Math.min(128, Number(event.target.value) || 1)),
                  )
                }
              />
            </div>
          </div>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={cleanBuild}
              onChange={(event) => onCleanChange(event.target.checked)}
            />
            Clean CMake output
          </label>
          <button
            className="primary"
            disabled={busy || reviewLoading || reviewHasMissingFiles(review)}
            onClick={() => onBuild(review)}
          >
            <Play size={16} /> Build{" "}
            {selectedPullRequests.length === 1 ? "selected PR" : "selected PRs"}
          </button>
        </div>
      )}
    </>
  );
}
