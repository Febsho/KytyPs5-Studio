import type { Build, Game, OfficialCompatibilityDetails, OfficialCompatibilityGame, OfficialCompatibilityIndex, OfficialCompatibilityReport, Repo } from "../types/studio";

export type SmartBuildRecommendation = {
  kind: "installed" | "build";
  build: Build | null;
  repo: Repo;
  report: OfficialCompatibilityReport;
  testedSha: string;
  status: string;
  explanation: string;
};

export type CompatibilityOS = "linux" | "windows" | "macos";

export function currentCompatibilityOS(): CompatibilityOS {
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent.toLowerCase();
  if (userAgent.includes("windows")) return "windows";
  if (userAgent.includes("macintosh") || userAgent.includes("mac os")) return "macos";
  return "linux";
}

const compatibilityStage = (status: string): number => {
  switch (status.trim().toLowerCase().replaceAll("_", "-").replaceAll(" ", "-")) {
    case "playable": return 5;
    case "in-game": return 4;
    case "main-menu": return 3;
    case "logo": return 2;
    case "doesnt-boot":
    case "does-not-boot": return 1;
    default: return 0;
  }
};

function oneExplicitCommit(value: string): string | null {
  const hashes = [...new Set(value.match(/[a-f0-9]{7,40}/gi)?.map((hash) => hash.toLowerCase()) ?? [])];
  return hashes.length === 1 ? hashes[0] : null;
}

function sameCommit(commit: string | null | undefined, testedSha: string): boolean {
  const candidate = commit?.trim().toLowerCase() ?? "";
  return candidate.length >= 7 && (candidate.startsWith(testedSha) || testedSha.startsWith(candidate));
}

/**
 * Only recommend builds with a host-OS report whose testedVersion names exactly
 * one source commit. Session exits, stars, PRs, and recent activity are never
 * used as compatibility or performance evidence.
 */
export function recommendVerifiedBuild(
  game: Game,
  details: OfficialCompatibilityDetails | null,
  builds: Build[],
  repos: Repo[],
  hostOS: CompatibilityOS = currentCompatibilityOS(),
): SmartBuildRecommendation | null {
  const titleId = normalizeTitleId(game.title_id);
  if (!titleId || !details) return null;

  const trackedRepos = repos.filter((repo) => !repo.removed);
  const latestReportsByCommit = new Map<string, OfficialCompatibilityReport>();
  for (const report of details.reports) {
    if (
      normalizeTitleId(report.titleId) !== titleId ||
      report.os.trim().toLowerCase() !== hostOS
    ) continue;
    const sha = oneExplicitCommit(report.testedVersion);
    if (!sha || compatibilityStage(report.status) <= 1) continue;
    const previous = latestReportsByCommit.get(sha);
    if (!previous || report.testedDate > previous.testedDate) latestReportsByCommit.set(sha, report);
  }

  const candidates: SmartBuildRecommendation[] = [];
  for (const [testedSha, report] of latestReportsByCommit) {
    for (const build of builds) {
      if (build.validation_status !== "verified" || build.executable_status !== "Executable present" || !sameCommit(build.commit_sha, testedSha)) continue;
      const repo = trackedRepos.find((entry) => entry.id === build.repo_id);
      if (!repo) continue;
      candidates.push({
        kind: "installed", build, repo, report, testedSha, status: report.status,
        explanation: `This installed build’s exact source commit is named by a ${hostOS} compatibility report.`,
      });
    }
    for (const repo of trackedRepos) {
      if (!repo.source_path || !sameCommit(repo.installed_commit, testedSha)) continue;
      if (candidates.some((candidate) => candidate.testedSha === testedSha && candidate.kind === "installed" && candidate.repo.id === repo.id)) continue;
      candidates.push({
        kind: "build", build: null, repo, report, testedSha, status: report.status,
        explanation: `This checked-out source commit is named by a ${hostOS} compatibility report; building it creates a candidate installation, not a guarantee of identical runtime behavior.`,
      });
    }
  }

  candidates.sort((a, b) =>
    compatibilityStage(b.status) - compatibilityStage(a.status) ||
    Number(b.kind === "installed") - Number(a.kind === "installed") ||
    b.report.testedDate.localeCompare(a.report.testedDate),
  );
  return candidates[0] ?? null;
}

export function normalizeTitleId(value: string | null | undefined): string | null {
  const normalized = value?.trim().toUpperCase().replaceAll("-", "") ?? "";
  return /^PPSA\d{5}$/.test(normalized) ? normalized : null;
}

export function officialCompatibilityForGame(
  game: Game,
  index: OfficialCompatibilityIndex | null,
): OfficialCompatibilityGame | null {
  const titleId = normalizeTitleId(game.title_id);
  if (!titleId || !index?.available) return null;
  return index.games.find((entry) => normalizeTitleId(entry.title_id) === titleId) ?? {
    title_id: titleId,
    title: game.title,
    overall: "untested",
    status: "untested",
    preferred_os: "all",
    os: {},
    report_counts: {},
    latest_tested: null,
  };
}

export function compatibilityStatusLabel(status: string | null | undefined): string {
  switch (status?.toLowerCase()) {
    case "doesnt-boot":
    case "doesntboot":
    case "doesn't boot":
      return "Not Booting";
    case "logo":
      return "Logo";
    case "main-menu":
    case "mainmenu":
    case "main menu":
      return "Main Menu";
    case "in-game":
    case "ingame":
    case "in game":
      return "In-Game";
    case "playable":
      return "Playable";
    default:
      return "Untested";
  }
}

export function compatibilityBadgeLabel(entry: OfficialCompatibilityGame | null): string {
  if (!entry) return "";
  const status = compatibilityStatusLabel(entry.status);
  if (entry.preferred_os === "linux") return `Linux · ${status}`;
  if (entry.preferred_os === "windows") return `Windows · ${status}`;
  if (entry.preferred_os === "macos") return `macOS · ${status}`;
  return Object.keys(entry.os).length ? `Overall · ${status}` : status;
}

export function compatibilityOsLabel(os: string): string {
  switch (os.toLowerCase()) {
    case "linux": return "Linux";
    case "windows": return "Windows";
    case "macos": return "macOS";
    default: return os || "Unknown platform";
  }
}
