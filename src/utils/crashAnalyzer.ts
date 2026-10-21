export type CrashCategoryId =
  | "imports"
  | "vulkan"
  | "shader"
  | "memory"
  | "configuration";

export type CrashFinding = {
  id: CrashCategoryId;
  title: string;
  summary: string;
  nextStep: string;
  evidence: string[];
};

const categories: Omit<CrashFinding, "evidence">[] = [
  {
    id: "imports",
    title: "Missing import or host library",
    summary: "The emulator or host loader reported an import, shared library, or symbol it could not resolve.",
    nextStep: "Include the unresolved symbol or library name and the game version. Check whether the host dependency is installed or another build provides the import.",
  },
  {
    id: "vulkan",
    title: "Vulkan or graphics device",
    summary: "The log contains a Vulkan loader, device, driver, or presentation failure.",
    nextStep: "Include the GPU model, driver version, Vulkan details, and whether the failure occurs with another graphics device.",
  },
  {
    id: "shader",
    title: "Shader or pipeline",
    summary: "The log contains shader compilation, SPIR-V, or graphics pipeline errors.",
    nextStep: "Include the shader error and the first game action that triggers it. Avoid deleting shader caches before saving the log.",
  },
  {
    id: "memory",
    title: "Memory access or process fault",
    summary: "The log reports an invalid memory access, access violation, or segmentation fault.",
    nextStep: "Include the fault address and the surrounding stack or log lines. A fault signature alone does not identify the root cause.",
  },
  {
    id: "configuration",
    title: "Invalid or unsupported configuration",
    summary: "The emulator rejected an option, value, or configuration file.",
    nextStep: "Include the full launch arguments and the selected emulator build. Remove only the option named in the error, then retry.",
  },
];

const patterns: Record<CrashCategoryId, RegExp> = {
  imports: /unresolved import|missing import|import stub|undefined symbol|symbol not found|error while loading shared libraries|cannot open shared object|DLL not found|failed to load (?:shared )?library/i,
  vulkan: /(?:vulkan|\bvk[A-Z]\w+|ICD|swapchain|physical device).{0,120}(?:error|failed|lost|unavailable|not found|unsupported)|(?:error|failed|lost|unavailable|not found|unsupported).{0,120}(?:vulkan|\bvk[A-Z]\w+|ICD|swapchain|physical device)|VK_ERROR_[A-Z_]+/i,
  shader: /(?:shader|SPIR-V|spirv|pipeline|glsl).{0,120}(?:error|failed|invalid|unsupported|exception)|(?:error|failed|invalid|unsupported|exception).{0,120}(?:shader|SPIR-V|spirv|pipeline|glsl)/i,
  memory: /segmentation fault|SIGSEGV|access violation|0xc0000005|page fault|invalid (?:memory )?access|unmapped address|memory fault/i,
  configuration: /unknown option|unsupported (?:option|argument)|invalid (?:option|argument|config(?:uration)?)|failed to parse.*(?:config|option)|expected .*argument/i,
};

export function analyzeEmulatorLog(log: string): CrashFinding[] {
  const lines = log.split(/\r?\n/);
  return categories.flatMap((category) => {
    const evidence: string[] = [];
    for (const line of lines) {
      if (!patterns[category.id].test(line)) continue;
      const excerpt = line.trim().slice(0, 600);
      if (excerpt && !evidence.includes(excerpt)) evidence.push(excerpt);
      if (evidence.length === 5) break;
    }
    return evidence.length ? [{ ...category, evidence }] : [];
  });
}

export function buildCrashReport(
  log: string,
  context: { game: string; build: string; startedAt: string },
): string {
  const findings = analyzeEmulatorLog(log);
  const sections = findings.length
    ? findings.map((finding) => [
        `### ${finding.title}`,
        finding.summary,
        `Suggested checks: ${finding.nextStep}`,
        "Evidence:",
        ...finding.evidence.map((line) => `> ${line.replaceAll("\n", " ")}`),
      ].join("\n\n"))
    : ["No known failure signature matched the selected log. The cause is undetermined."];
  return [
    "# KytyPS5 Studio crash analysis",
    "",
    `- Game: ${context.game}`,
    `- Emulator build: ${context.build}`,
    `- Session started: ${context.startedAt}`,
    "",
    "> This report contains matched log excerpts only. Findings are hints, not a confirmed root cause.",
    "",
    ...sections,
    "",
  ].join("\n");
}
