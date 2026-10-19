// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CommunityPR } from "../../types/studio";
import { CommunityPRTable } from "./CommunityPRTable";

const pr: CommunityPR = {
  number: 23,
  title: "Improve command line help",
  body: "",
  state: "open",
  merged: false,
  draft: true,
  updated_at: "2026-10-12T00:00:00Z",
  created_at: "2026-10-01T00:00:00Z",
  html_url: "https://github.com/KytyPS5/KytyPS5/pull/23",
  author: "dev",
  base_repo: "KytyPS5/KytyPS5",
  base_branch: "main",
  base_sha: "abcdef123456",
  head_repo: "dev/KytyPS5",
  head_branch: "cli-help",
  head_sha: "0123456789ab",
  labels: ["cli", "docs"],
  mergeable: true,
  mergeable_state: "clean",
  changed_files: 2,
  additions: 5,
  deletions: 1,
  build_compatibility: "Unknown",
};

afterEach(cleanup);

describe("CommunityPRTable", () => {
  it("renders PR details and delegates selection and detail actions", () => {
    const onSelectionChange = vi.fn();
    const onOpenDetails = vi.fn();
    render(
      <CommunityPRTable
        pullRequests={[pr]}
        selectedPullRequests={[]}
        selectedDetails={null}
        loading={false}
        onSelectionChange={onSelectionChange}
        onOpenDetails={onOpenDetails}
      />,
    );

    expect(screen.getByRole("link", { name: "#23" })).toBeTruthy();
    expect(
      screen.getByText("dev/KytyPS5:cli-help → main · Draft"),
    ).toBeTruthy();
    expect(screen.getByText("cli")).toBeTruthy();
    fireEvent.click(screen.getByRole("checkbox", { name: "Select PR 23" }));
    fireEvent.click(
      screen.getByRole("button", { name: /Improve command line help/ }),
    );
    expect(onSelectionChange).toHaveBeenCalledWith(pr, true);
    expect(onOpenDetails).toHaveBeenCalledWith(pr);
  });

  it("shows loading and empty-result states", () => {
    const { rerender } = render(
      <CommunityPRTable
        pullRequests={[]}
        selectedPullRequests={[]}
        selectedDetails={null}
        loading
        onSelectionChange={vi.fn()}
        onOpenDetails={vi.fn()}
      />,
    );
    expect(screen.getByRole("status")).toBeTruthy();
    expect(screen.queryByText("No pull requests match")).toBeNull();

    rerender(
      <CommunityPRTable
        pullRequests={[]}
        selectedPullRequests={[]}
        selectedDetails={null}
        loading={false}
        onSelectionChange={vi.fn()}
        onOpenDetails={vi.fn()}
      />,
    );
    expect(screen.getByText("No pull requests match")).toBeTruthy();
  });
});
