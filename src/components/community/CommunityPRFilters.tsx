import type { Repo } from "../../types/studio";
import type { CommunityPRSort, CommunityPRState } from "./communityModel";

export type { CommunityPRSort, CommunityPRState } from "./communityModel";

export function CommunityPRFilters({
  repos,
  selectedRepo,
  state,
  search,
  number,
  label,
  mergedOnly,
  sort,
  onRepositoryChange,
  onStateChange,
  onSearchChange,
  onNumberChange,
  onLabelChange,
  onMergedOnlyChange,
  onSortChange,
}: {
  repos: Repo[];
  selectedRepo: number | null;
  state: CommunityPRState;
  search: string;
  number: string;
  label: string;
  mergedOnly: boolean;
  sort: CommunityPRSort;
  onRepositoryChange: (id: number | null) => void;
  onStateChange: (state: CommunityPRState) => void;
  onSearchChange: (search: string) => void;
  onNumberChange: (number: string) => void;
  onLabelChange: (label: string) => void;
  onMergedOnlyChange: (enabled: boolean) => void;
  onSortChange: (sort: CommunityPRSort) => void;
}) {
  return (
    <div className="community-toolbar">
      <label>
        Repository
        <select
          value={selectedRepo ?? ""}
          onChange={(event) =>
            onRepositoryChange(Number(event.target.value) || null)
          }
        >
          {repos.map((repo) => (
            <option key={repo.id} value={repo.id}>
              {repo.owner}/{repo.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        State
        <select
          value={state}
          onChange={(event) =>
            onStateChange(event.target.value as CommunityPRState)
          }
        >
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          <option value="all">All</option>
        </select>
      </label>
      <label>
        Search all repository PRs
        <input
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder="Title, description, or author"
        />
      </label>
      <label>
        PR number
        <input
          type="number"
          min="1"
          value={number}
          onChange={(event) => onNumberChange(event.target.value)}
          placeholder="#123"
        />
      </label>
      <label>
        Label
        <input
          value={label}
          onChange={(event) => onLabelChange(event.target.value)}
          placeholder="Filter label"
        />
      </label>
      <label className="checkbox">
        <input
          type="checkbox"
          checked={mergedOnly}
          onChange={(event) => onMergedOnlyChange(event.target.checked)}
        />
        Merged only
      </label>
      <label>
        Sort
        <select
          value={sort}
          onChange={(event) =>
            onSortChange(event.target.value as CommunityPRSort)
          }
        >
          <option value="updated">Recently updated</option>
          <option value="newest">Newest PR</option>
          <option value="number">PR number</option>
          <option value="author">Author</option>
        </select>
      </label>
    </div>
  );
}
