import type { CommunityPR } from "../../types/studio";
export type CommunityPRState = "open" | "closed" | "all";
export type CommunityPRSort = "updated" | "newest" | "number" | "author";

export type CommunityPRFiltersState = {
  search: string;
  number: string;
  label: string;
  mergedOnly: boolean;
  sort: CommunityPRSort;
};

export function filterAndSortCommunityPRs(
  pullRequests: CommunityPR[],
  filters: CommunityPRFiltersState,
): CommunityPR[] {
  const search = filters.search.trim().toLowerCase();
  const label = filters.label.trim().toLowerCase();
  return pullRequests
    .filter((pr) => {
      const matchesSearch =
        !search ||
        pr.title.toLowerCase().includes(search) ||
        pr.author.toLowerCase().includes(search) ||
        (pr.body ?? "").toLowerCase().includes(search);
      return (
        matchesSearch &&
        (!filters.number || String(pr.number).includes(filters.number)) &&
        (!filters.mergedOnly || pr.merged) &&
        (!label || pr.labels.some((item) => item.toLowerCase().includes(label)))
      );
    })
    .sort((a, b) =>
      filters.sort === "number"
        ? b.number - a.number
        : filters.sort === "author"
          ? a.author.localeCompare(b.author)
          : filters.sort === "newest"
            ? b.created_at.localeCompare(a.created_at)
            : b.updated_at.localeCompare(a.updated_at),
    );
}
