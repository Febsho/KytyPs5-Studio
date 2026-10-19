// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  CommunityBuildHistory as Entry,
  CommunityPRPage,
} from "../../types/studio";
import { CommunityBuildHistory } from "./CommunityBuildHistory";

const entry: Entry = {
  result_id: 11,
  config_id: 4,
  name: "Vulkan diagnostics",
  repository: "KytyPS5/KytyPS5",
  base_branch: "main",
  base_sha: "abcdef1234567890",
  build_type: "Release",
  jobs: 8,
  compiler: "clang",
  clean: true,
  pull_requests: [[42, "0123456789abcdef"]],
  build_id: 5,
  install_path: "/data/experimental/build-11",
  status: "verified",
  started_at: 1790812800,
  finished_at: 1790816400,
  error: null,
  active: false,
};

const pullRequests: CommunityPRPage = {
  repository: "KytyPS5/KytyPS5",
  items: [
    {
      number: 42,
      title: "Vulkan diagnostics",
      body: null,
      state: "open",
      merged: false,
      draft: false,
      updated_at: "2026-10-01T00:00:00Z",
      created_at: "2026-09-01T00:00:00Z",
      html_url: "https://github.com/KytyPS5/KytyPS5/pull/42",
      author: "contributor",
      base_repo: "KytyPS5/KytyPS5",
      base_branch: "main",
      base_sha: "abcdef1234567890",
      head_repo: "contributor/KytyPS5",
      head_branch: "vulkan-diagnostics",
      head_sha: "fedcba9876543210",
      labels: [],
      mergeable: true,
      mergeable_state: "clean",
      changed_files: 1,
      additions: 2,
      deletions: 0,
      build_compatibility: "Unknown",
    },
  ],
  total_count: 1,
  page: 1,
  per_page: 50,
  has_more: false,
  truncated: false,
  stale: false,
  refresh_error: null,
};

afterEach(cleanup);

describe("CommunityBuildHistory", () => {
  it("shows changed PR heads and delegates history actions", () => {
    const onActivate = vi.fn();
    const onReview = vi.fn();
    const onRename = vi.fn();
    render(
      <CommunityBuildHistory
        history={[entry]}
        pullRequests={pullRequests}
        busy={false}
        reviewLoading={false}
        onActivate={onActivate}
        onReview={onReview}
        onRename={onRename}
      />,
    );

    expect(screen.getByText(/PR commits updated on GitHub/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Activate" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Rebuild / duplicate" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(onActivate).toHaveBeenCalledWith(5);
    expect(onReview).toHaveBeenCalledWith(entry);
    expect(onRename).toHaveBeenCalledWith(entry);
  });

  it("explains the empty state and disables rebuild during a job", () => {
    const { rerender } = render(
      <CommunityBuildHistory
        history={[]}
        pullRequests={null}
        busy={false}
        reviewLoading={false}
        onActivate={vi.fn()}
        onReview={vi.fn()}
        onRename={vi.fn()}
      />,
    );
    expect(screen.getByText("No community builds yet")).toBeTruthy();

    rerender(
      <CommunityBuildHistory
        history={[entry]}
        pullRequests={null}
        busy
        reviewLoading={false}
        onActivate={vi.fn()}
        onReview={vi.fn()}
        onRename={vi.fn()}
      />,
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Rebuild / duplicate",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
  });
});
