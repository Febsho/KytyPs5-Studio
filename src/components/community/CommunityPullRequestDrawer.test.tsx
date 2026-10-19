// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CommunityPRDetails } from "../../types/studio";
import { CommunityPullRequestDrawer } from "./CommunityPullRequestDrawer";

const details: CommunityPRDetails = {
  pull_request: {
    number: 42,
    title: "Improve Vulkan reporting",
    body: "Adds device diagnostics.",
    state: "open",
    merged: false,
    draft: false,
    updated_at: "2026-10-01T00:00:00Z",
    created_at: "2026-09-01T00:00:00Z",
    html_url: "https://github.com/example/project/pull/42",
    author: "contributor",
    base_repo: "example/project",
    base_branch: "main",
    base_sha: "abcdef1234567890",
    head_repo: "contributor/project",
    head_branch: "gpu-reporting",
    head_sha: "0123456789abcdef",
    labels: [],
    mergeable: false,
    mergeable_state: "dirty",
    changed_files: 1,
    additions: 3,
    deletions: 1,
    build_compatibility: "Unknown",
  },
  commits: [
    {
      sha: "0123456789abcdef",
      commit: { message: "Add device name", author: null },
    },
  ],
  files: [
    {
      filename: "src/graphics.cpp",
      status: "modified",
      additions: 3,
      deletions: 1,
      patch: "+ device name",
    },
  ],
  checks: [
    {
      name: "CI",
      status: "completed",
      conclusion: "success",
      html_url: "https://github.com/example/project/actions",
    },
  ],
  checks_state: "success",
  subresource_errors: [],
};

afterEach(cleanup);

describe("CommunityPullRequestDrawer", () => {
  it("shows PR metadata, checks, commits and patches and closes on request", () => {
    const onClose = vi.fn();
    render(<CommunityPullRequestDrawer details={details} onClose={onClose} />);

    expect(
      screen.getByRole("heading", { name: /#42 Improve Vulkan/ }),
    ).toBeTruthy();
    expect(screen.getByText("contributor · gpu-reporting → main")).toBeTruthy();
    expect(screen.getByText("Conflicting · dirty")).toBeTruthy();
    expect(screen.getByText("CI")).toBeTruthy();
    expect(screen.getByText("Add device name")).toBeTruthy();
    expect(screen.getByText("+ device name")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("does not render when there is no selected PR", () => {
    const { container } = render(
      <CommunityPullRequestDrawer details={null} onClose={vi.fn()} />,
    );
    expect(container.innerHTML).toBe("");
  });
});
