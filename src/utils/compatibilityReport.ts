export type CompatibilityIssueLink = {
  number: number;
  title: string;
  state: string;
};

export type CompatibilityIssueDraft = {
  gameTitle: string;
  titleId: string;
  gameVersion: string;
  kytyVersion: string;
  status: string;
  details: string;
  steps: string;
  expected: string;
  os: string;
  cpu: string;
  gpu: string;
  memory: string;
  patchConfig: string;
  extraNotes: string;
  relatedIssues: CompatibilityIssueLink[];
  log: string;
};

function escapeMarkdown(value: string): string {
  return value.replace(/[\\`*_{}\[\]()#+\-.!|<>]/g, "\\$&");
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

export function compatibilityIssueBody(draft: CompatibilityIssueDraft): string {
  const lines = [
    "## Game and build",
    `- **Game title:** ${escapeMarkdown(draft.gameTitle)}`,
    `- **Game ID / serial:** ${escapeMarkdown(draft.titleId || "Unknown")}`,
    `- **Game version:** ${escapeMarkdown(draft.gameVersion || "Not reported")}`,
    `- **KytyPS5 version:** ${escapeMarkdown(draft.kytyVersion || "Unknown")}`,
    `- **Compatibility status:** ${escapeMarkdown(draft.status)}`,
    "",
    "## Test result",
    "### Result details",
    escapeMarkdown(draft.details),
    "",
    "### Steps to reproduce",
    escapeMarkdown(draft.steps),
    "",
    "### Expected behavior",
    escapeMarkdown(draft.expected),
    "",
    "## Host system",
    `- **OS:** ${escapeMarkdown(draft.os || "Not reported")}`,
    `- **CPU:** ${escapeMarkdown(draft.cpu || "Not reported")}`,
    `- **GPU:** ${escapeMarkdown(draft.gpu || "Not reported")}`,
    `- **RAM / VRAM:** ${escapeMarkdown(draft.memory || "Not reported")}`,
  ];

  if (draft.patchConfig.trim()) {
    lines.push(
      "",
      "## Build and patch settings recorded with the observation",
      "```json",
      draft.patchConfig.slice(0, 12_000),
      "```",
    );
  }

  if (draft.relatedIssues.length) {
    lines.push("", "## Related existing issues");
    for (const issue of draft.relatedIssues) {
      lines.push(
        `- [#${issue.number}: ${escapeMarkdown(issue.title)}](https://github.com/KytyPS5/KytyPS5/issues/${issue.number})`,
      );
    }
  }

  if (draft.log.trim()) {
    lines.push(
      "",
      "## Complete unmodified emulator log",
      "<details>",
      "<summary>Open log (may contain local file paths and system information)</summary>",
      "",
      `<pre>${escapeHtml(draft.log)}</pre>`,
      "</details>",
    );
  } else {
    lines.push("", "## Log file upload", "No log was included. Please attach the complete KytyPS5 log to this issue.");
  }

  lines.push("", "## Screenshots/video", "No media attached.", "", "## Extra notes", escapeMarkdown(draft.extraNotes || "Report prepared by KytyPS5 Studio; review the details before relying on this result."));
  return lines.join("\n");
}

export function compatibilityIssueTitle(gameTitle: string, titleId: string): string {
  return `[GAME STATUS]: ${gameTitle.trim()}${titleId ? ` (${titleId})` : ""}`.slice(0, 256);
}

export function compatibilityStatusFromObservation(outcome: string): string {
  switch (outcome) {
    case "launch_failed": return "Doesn't boot";
    case "reached_menu": return "Main menu";
    case "reached_gameplay":
    case "playable":
    case "playable_with_issues": return "In game";
    default: return "";
  }
}
