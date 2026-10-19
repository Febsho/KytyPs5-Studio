import type { CommunityPR, CommunityPRDetails } from "../../types/studio";

export function CommunityPRTable({
  pullRequests,
  selectedPullRequests,
  selectedDetails,
  loading,
  onSelectPage = () => {},
  onSelectionChange,
  onOpenDetails,
}: {
  pullRequests: CommunityPR[];
  selectedPullRequests: CommunityPR[];
  selectedDetails: CommunityPRDetails | null;
  loading: boolean;
  onSelectPage?: (selected: boolean) => void;
  onSelectionChange: (pr: CommunityPR, selected: boolean) => void;
  onOpenDetails: (pr: CommunityPR) => void;
}) {
  const selectedOnPage = pullRequests.filter((pr) =>
    selectedPullRequests.some((item) => item.number === pr.number),
  ).length;
  return (
    <div className="pr-table-wrap">
      <table className="pr-table">
        <thead>
          <tr>
            <th>
              <label className="pr-select-page">
                <input
                  type="checkbox"
                  aria-label="Select all pull requests on this page"
                  checked={
                    pullRequests.length > 0 &&
                    selectedOnPage === pullRequests.length
                  }
                  ref={(element) => {
                    if (element)
                      element.indeterminate =
                        selectedOnPage > 0 &&
                        selectedOnPage < pullRequests.length;
                  }}
                  onChange={(event) => onSelectPage(event.target.checked)}
                />
                <span>Select</span>
              </label>
            </th>
            <th>PR</th>
            <th>Title / branch</th>
            <th>Author</th>
            <th>Labels</th>
            <th>Status</th>
            <th>Updated</th>
            <th>Changes</th>
            <th>Compatibility</th>
          </tr>
        </thead>
        <tbody>
          {pullRequests.map((pr) => (
            <tr
              key={pr.number}
              className={
                selectedDetails?.pull_request.number === pr.number
                  ? "selected"
                  : ""
              }
            >
              <td>
                <input
                  type="checkbox"
                  aria-label={`Select PR ${pr.number}`}
                  checked={selectedPullRequests.some(
                    (item) => item.number === pr.number,
                  )}
                  onChange={(event) =>
                    onSelectionChange(pr, event.target.checked)
                  }
                />
              </td>
              <td>
                <a href={pr.html_url} target="_blank" rel="noreferrer">
                  #{pr.number}
                </a>
                {pr.merged && <span className="badge">Merged</span>}
              </td>
              <td>
                <button className="pr-title" onClick={() => onOpenDetails(pr)}>
                  <strong>{pr.title}</strong>
                  <small>
                    {pr.head_repo || "deleted fork"}:{pr.head_branch} →{" "}
                    {pr.base_branch}
                    {pr.draft ? " · Draft" : ""}
                  </small>
                </button>
              </td>
              <td>{pr.author}</td>
              <td>
                {pr.labels.length
                  ? pr.labels.map((label) => (
                      <span className="label-chip" key={label}>
                        {label}
                      </span>
                    ))
                  : "—"}
              </td>
              <td>
                <span
                  className={`badge ${pr.state === "open" ? "success" : ""}`}
                >
                  {pr.merged ? "Merged" : pr.state}
                </span>
              </td>
              <td>{new Date(pr.updated_at).toLocaleDateString()}</td>
              <td>
                {pr.changed_files ?? "—"} files · +{pr.additions ?? "—"} / −
                {pr.deletions ?? "—"}
              </td>
              <td>{pr.build_compatibility}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {loading && (
        <p className="hint" role="status">
          Loading repository pull requests from GitHub…
        </p>
      )}
      {!loading && !pullRequests.length && (
        <div className="empty compact">
          <strong>No pull requests match</strong>
          <p>Try another search, label, or state.</p>
        </div>
      )}
    </div>
  );
}
