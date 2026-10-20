// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { BuildStudioPageProps } from "./BuildStudioPage";
import { BuildStudioPage } from "./BuildStudioPage";
import type { Repo } from "../types/studio";

const repo: Repo = {
  id: 7,
  owner: "KytyPS5",
  name: "KytyPS5",
  url: "https://github.com/KytyPS5/KytyPS5",
  branch: "main",
  source_path: "/tmp/kyty-source",
  adapter: "official",
  installed_commit: "abc123",
};

function props(
  overrides: Partial<BuildStudioPageProps> = {},
): BuildStudioPageProps {
  return {
    repos: [repo],
    currentRepo: repo,
    buildSourceInfo: null,
    system: null,
    buildMode: "simple",
    setBuildMode: vi.fn(),
    buildType: "Release",
    setBuildType: vi.fn(),
    jobs: 4,
    setJobs: vi.fn(),
    cleanBuild: false,
    setCleanBuild: vi.fn(),
    busy: false,
    currentBuildPhase: "Idle",
    elapsedSeconds: 0,
    logs: [],
    onSelectRepo: vi.fn(),
    onBuild: vi.fn(),
    onBuildLatest: vi.fn(),
    onInstallTools: vi.fn(),
    onCancel: vi.fn(),
    ...overrides,
  };
}

afterEach(cleanup);

describe("BuildStudioPage", () => {
  it("switches between simple and advanced build workflows", () => {
    const page = props();
    const { rerender } = render(<BuildStudioPage {...page} />);

    fireEvent.click(screen.getByRole("button", { name: "Advanced" }));
    expect(page.setBuildMode).toHaveBeenCalledWith("advanced");
    rerender(<BuildStudioPage {...page} buildMode="advanced" />);
    expect(screen.getByText("Submodule workflow")).toBeTruthy();
  });

  it("starts a build for the selected repository and keeps build stage visible", () => {
    const page = props({ currentBuildPhase: "Configuring CMake" });
    const { container } = render(<BuildStudioPage {...page} />);

    fireEvent.click(container.querySelector(".build-main-action")!);
    expect(page.onBuild).toHaveBeenCalledOnce();
    expect(
      container.querySelector(".pipeline-steps .current")?.textContent,
    ).toContain("Configuring CMake");
  });

  it("offers a tool installer and blocks builds when Linux build requirements are missing", () => {
    const page = props({
      system: {
        os: "Ubuntu Linux",
        arch: "x86_64",
        cpu: "CPU",
        memory_total_bytes: 0,
        memory_available_bytes: 0,
        controllers: [],
        session: "wayland",
        gpu: "GPU",
        dependencies: [
          { name: "cmake", present: true },
          { name: "clang", present: false },
          { name: "vulkaninfo", present: false },
        ],
        paths: {},
      },
    });
    const { container } = render(<BuildStudioPage {...page} />);

    expect(screen.getByText(/Missing build tools: clang/)).toBeTruthy();
    expect(screen.queryByText(/vulkaninfo/)).toBeNull();
    expect((container.querySelector(".build-main-action") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Install missing tools" }));
    expect(page.onInstallTools).toHaveBeenCalledOnce();
  });
});
