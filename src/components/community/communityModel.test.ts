import { describe, expect, it } from "vitest";
import type { CommunityPR } from "../../types/studio";
import { filterAndSortCommunityPRs } from "./communityModel";

const pullRequests: CommunityPR[] = [
  {
    number: 12,
    title: "Update graphics diagnostics",
    body: "Reports Vulkan device details.",
    state: "closed",
    merged: true,
    draft: false,
    updated_at: "2026-10-02T00:00:00Z",
    created_at: "2026-09-02T00:00:00Z",
    html_url: "https://github.com/example/project/pull/12",
    author: "zeta",
    base_repo: "example/project",
    base_branch: "main",
    base_sha: "base",
    head_repo: "zeta/project",
    head_branch: "graphics",
    head_sha: "head12",
    labels: ["graphics", "vulkan"],
    mergeable: true,
    mergeable_state: "clean",
    changed_files: 2,
    additions: 3,
    deletions: 1,
    build_compatibility: "Unknown",
  },
  {
    number: 8,
    title: "Fix startup notice",
    body: "",
    state: "open",
    merged: false,
    draft: false,
    updated_at: "2026-10-03T00:00:00Z",
    created_at: "2026-09-04T00:00:00Z",
    html_url: "https://github.com/example/project/pull/8",
    author: "alpha",
    base_repo: "example/project",
    base_branch: "main",
    base_sha: "base",
    head_repo: "alpha/project",
    head_branch: "notice",
    head_sha: "head8",
    labels: ["ui"],
    mergeable: true,
    mergeable_state: "clean",
    changed_files: 1,
    additions: 2,
    deletions: 2,
    build_compatibility: "Unknown",
  },
  {
    number: 31,
    title: "Document controller mapping",
    body: "Adds examples for keymap values.",
    state: "closed",
    merged: true,
    draft: false,
    updated_at: "2026-10-01T00:00:00Z",
    created_at: "2026-09-03T00:00:00Z",
    html_url: "https://github.com/example/project/pull/31",
    author: "beta",
    base_repo: "example/project",
    base_branch: "main",
    base_sha: "base",
    head_repo: "beta/project",
    head_branch: "keymap-docs",
    head_sha: "head31",
    labels: ["docs"],
    mergeable: true,
    mergeable_state: "clean",
    changed_files: 1,
    additions: 7,
    deletions: 0,
    build_compatibility: "Unknown",
  },
];

const defaults = {
  search: "",
  number: "",
  label: "",
  mergedOnly: false,
  sort: "updated" as const,
};

describe("community pull request filtering", () => {
  it("searches titles, bodies and authors and combines number and label filters", () => {
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        search: " VULKAN ",
        number: "12",
        label: "graph",
      }).map((pr) => pr.number),
    ).toEqual([12]);
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        search: "beta",
      }).map((pr) => pr.number),
    ).toEqual([31]);
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        search: "keymap values",
      }).map((pr) => pr.number),
    ).toEqual([31]);
  });

  it("filters merged requests and supports each sort without mutating inputs", () => {
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        mergedOnly: true,
        sort: "number",
      }).map((pr) => pr.number),
    ).toEqual([31, 12]);
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        sort: "author",
      }).map((pr) => pr.author),
    ).toEqual(["alpha", "beta", "zeta"]);
    expect(
      filterAndSortCommunityPRs(pullRequests, {
        ...defaults,
        sort: "newest",
      }).map((pr) => pr.number),
    ).toEqual([8, 31, 12]);
    expect(pullRequests.map((pr) => pr.number)).toEqual([12, 8, 31]);
  });
});
