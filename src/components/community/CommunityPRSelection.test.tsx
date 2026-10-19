// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CommunityPR, CommunityPRDetails } from "../../types/studio";
import { CommunityPRSelection } from "./CommunityPRSelection";

const pr: CommunityPR = {
  number: 7,
  title: "Update Vulkan setup",
  body: null,
  state: "open",
  merged: false,
  draft: false,
  updated_at: "2026-10-01T00:00:00Z",
  created_at: "2026-09-01T00:00:00Z",
  html_url: "https://github.com/KytyPS5/KytyPS5/pull/7",
  author: "dev",
  base_repo: "KytyPS5/KytyPS5",
  base_branch: "main",
  base_sha: "abcdef123456",
  head_repo: "dev/KytyPS5",
  head_branch: "vulkan-setup",
  head_sha: "0123456789ab",
  labels: [],
  mergeable: true,
  mergeable_state: "clean",
  changed_files: 1,
  additions: 1,
  deletions: 0,
  build_compatibility: "Unknown",
};

const detail: CommunityPRDetails = {
  pull_request: pr,
  commits: [],
  files: [
    {
      filename: "src/vulkan.cpp",
      status: "modified",
      additions: 1,
      deletions: 0,
      patch: "+ setup",
    },
  ],
  checks: [],
  checks_state: "success",
  subresource_errors: [],
};
const secondPR: CommunityPR = {
  ...pr,
  number: 8,
  title: "Add alternate device name",
};
const secondDetail: CommunityPRDetails = {
  ...detail,
  pull_request: secondPR,
};

afterEach(cleanup);

function props(
  overrides: Partial<Parameters<typeof CommunityPRSelection>[0]> = {},
) {
  return {
    selectedPullRequests: [pr],
    review: [detail],
    pinnedHeads: {},
    busy: false,
    reviewLoading: false,
    buildType: "Release",
    jobs: 4,
    cleanBuild: false,
    onMove: vi.fn(),
    onClear: vi.fn(),
    onReview: vi.fn(),
    onBuild: vi.fn(),
    onBuildTypeChange: vi.fn(),
    onJobsChange: vi.fn(),
    onCleanChange: vi.fn(),
    ...overrides,
  };
}

describe("CommunityPRSelection", () => {
  it("shows reviewed files and delegates ordering, review, and build actions", () => {
    const onMove = vi.fn();
    const onClear = vi.fn();
    const onReview = vi.fn();
    const onBuild = vi.fn();
    render(
      <CommunityPRSelection
        {...props({
          selectedPullRequests: [pr, secondPR],
          review: [detail, secondDetail],
          onMove,
          onClear,
          onReview,
          onBuild,
        })}
      />,
    );

    expect(screen.getAllByText("src/vulkan.cpp")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Move PR 7 later" }));
    fireEvent.click(screen.getByRole("button", { name: "Review selection" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Build selected PRs" }),
    );
    expect(onMove).toHaveBeenCalledWith(0, 1);
    expect(onReview).toHaveBeenCalledOnce();
    expect(onClear).toHaveBeenCalledOnce();
    expect(onBuild).toHaveBeenCalledWith([detail, secondDetail]);
  });

  it("blocks compilation when GitHub did not return every changed file", () => {
    const incomplete = {
      ...detail,
      files: [],
      pull_request: { ...pr, changed_files: 2 },
    };
    render(<CommunityPRSelection {...props({ review: [incomplete] })} />);
    expect(
      (
        screen.getByRole("button", {
          name: "Build selected PR",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText(/GitHub reported 2 changed files/)).toBeTruthy();
  });
});
