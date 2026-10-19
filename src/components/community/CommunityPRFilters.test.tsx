// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Repo } from "../../types/studio";
import { CommunityPRFilters } from "./CommunityPRFilters";

const repos: Repo[] = [
  {
    id: 8,
    owner: "KytyPS5",
    name: "KytyPS5",
    url: "https://github.com/KytyPS5/KytyPS5",
    branch: "main",
    source_path: "/tmp/kyty",
    adapter: "official",
    installed_commit: null,
  },
];

afterEach(cleanup);

describe("CommunityPRFilters", () => {
  it("exposes the repository, search and sorting controls", () => {
    const handlers = {
      onRepositoryChange: vi.fn(),
      onStateChange: vi.fn(),
      onSearchChange: vi.fn(),
      onNumberChange: vi.fn(),
      onLabelChange: vi.fn(),
      onMergedOnlyChange: vi.fn(),
      onSortChange: vi.fn(),
    };
    render(
      <CommunityPRFilters
        repos={repos}
        selectedRepo={8}
        state="open"
        search=""
        number=""
        label=""
        mergedOnly={false}
        sort="updated"
        {...handlers}
      />,
    );

    fireEvent.change(screen.getByLabelText("Repository"), {
      target: { value: "8" },
    });
    fireEvent.change(screen.getByLabelText("State"), {
      target: { value: "closed" },
    });
    fireEvent.change(screen.getByLabelText("Search all repository PRs"), {
      target: { value: "vulkan" },
    });
    fireEvent.change(screen.getByLabelText("PR number"), {
      target: { value: "42" },
    });
    fireEvent.change(screen.getByLabelText("Label"), {
      target: { value: "graphics" },
    });
    fireEvent.click(screen.getByLabelText("Merged only"));
    fireEvent.change(screen.getByLabelText("Sort"), {
      target: { value: "number" },
    });

    expect(handlers.onRepositoryChange).toHaveBeenCalledWith(8);
    expect(handlers.onStateChange).toHaveBeenCalledWith("closed");
    expect(handlers.onSearchChange).toHaveBeenCalledWith("vulkan");
    expect(handlers.onNumberChange).toHaveBeenCalledWith("42");
    expect(handlers.onLabelChange).toHaveBeenCalledWith("graphics");
    expect(handlers.onMergedOnlyChange).toHaveBeenCalledWith(true);
    expect(handlers.onSortChange).toHaveBeenCalledWith("number");
  });
});
