// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RepositoryManagerPageProps } from "./RepositoryManagerPage";
import { RepositoryManagerPage } from "./RepositoryManagerPage";
import type { Repo } from "../types/studio";

afterEach(cleanup);

const repo: Repo = {
  id: 6,
  owner: "example",
  name: "Kyty fork",
  url: "https://github.com/example/kyty-fork",
  branch: "main",
  source_path: "/tmp/kyty-fork",
  adapter: "kyty-cmake",
  installed_commit: "abc123456789",
};

function props(
  overrides: Partial<RepositoryManagerPageProps> = {},
): RepositoryManagerPageProps {
  return {
    forks: [],
    repos: [repo],
    currentRepo: repo,
    selectedRepo: repo.id,
    repoUrl: "",
    setRepoUrl: vi.fn(),
    githubDetails: null,
    busy: false,
    onDiscoverForks: vi.fn(),
    onAddManualRepository: vi.fn(),
    onAddOfficialRepository: vi.fn(),
    onSelectRepository: vi.fn(),
    onCloneRepository: vi.fn(),
    onOpenSourceFolder: vi.fn(),
    onInspectRepository: vi.fn(),
    onCheckRepositoryUpdates: vi.fn(),
    onRefreshGitHubDetails: vi.fn(),
    onOpenBuildStudio: vi.fn(),
    onAddFork: vi.fn(),
    onRemoveRepository: vi.fn(),
    onUpdateNow: vi.fn(),
    ...overrides,
  };
}

describe("RepositoryManagerPage", () => {
  it("forwards repository discovery, entry and official import actions", () => {
    const page = props({ currentRepo: undefined, repos: [] });
    render(<RepositoryManagerPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Discover forks" }));
    expect(page.onDiscoverForks).toHaveBeenCalledOnce();
    fireEvent.change(
      screen.getByPlaceholderText("https://github.com/owner/repository"),
      {
        target: { value: "https://github.com/example/new" },
      },
    );
    expect(page.setRepoUrl).toHaveBeenCalledWith(
      "https://github.com/example/new",
    );
    fireEvent.click(screen.getByRole("button", { name: "Add repository" }));
    expect(page.onAddManualRepository).toHaveBeenCalledOnce();
    fireEvent.click(
      screen.getByRole("button", { name: "Select Official Source" }),
    );
    expect(page.onAddOfficialRepository).toHaveBeenCalledOnce();
  });

  it("removes a tracked fork immediately without a confirmation popup", () => {
    const page = props();
    const confirm = vi.spyOn(window, "confirm");
    render(<RepositoryManagerPage {...page} />);
    fireEvent.click(screen.getByRole("button", { name: "Remove example/Kyty fork" }));
    expect(page.onRemoveRepository).toHaveBeenCalledWith(repo.id);
    expect(confirm).not.toHaveBeenCalled();
  });

  it("keeps source management actions tied to the selected repository", () => {
    const page = props();
    const cloningRepo = { ...repo, source_path: null };
    const { rerender } = render(
      <RepositoryManagerPage
        {...page}
        repos={[cloningRepo]}
        currentRepo={cloningRepo}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Clone / restore checkout" }),
    );
    expect(page.onCloneRepository).toHaveBeenCalledWith(repo.id);
    rerender(<RepositoryManagerPage {...page} />);
    fireEvent.click(screen.getByRole("button", { name: "Inspect adapter" }));
    expect(page.onInspectRepository).toHaveBeenCalledWith(repo.id);
    fireEvent.click(screen.getByRole("button", { name: "Build" }));
    expect(page.onOpenBuildStudio).toHaveBeenCalledOnce();
  });

  it("disables clone and restore while another job is active", () => {
    render(<RepositoryManagerPage {...props({ busy: true })} />);

    expect(
      (screen.getByRole("button", {
        name: "Clone / restore checkout",
      }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("shows installed and available commits and starts the selected repository update", async () => {
    const page = props({
      onCheckRepositoryUpdates: vi
        .fn()
        .mockResolvedValue("Update available: abc123456789 → def987654321"),
    });
    render(<RepositoryManagerPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Check updates" }));
    expect(await screen.findByText("A newer build is available")).toBeTruthy();
    expect(screen.getAllByText("abc123456789").length).toBeGreaterThan(0);
    expect(screen.getByText("def987654321")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Fetch latest and build" }));
    expect(page.onUpdateNow).toHaveBeenCalledWith(repo.id);
  });

  it("disables update checks while another task is running", () => {
    render(<RepositoryManagerPage {...props({ busy: true })} />);
    expect(
      (screen.getByRole("button", { name: "Check updates" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
