import { describe, expect, it } from "vitest";
import { compatibilityIssueBody, compatibilityIssueTitle, compatibilityStatusFromObservation } from "./compatibilityReport";

describe("compatibility report generation", () => {
  it("maps only saved manual outcomes to the official status vocabulary", () => {
    expect(compatibilityStatusFromObservation("launch_failed")).toBe("Doesn't boot");
    expect(compatibilityStatusFromObservation("reached_menu")).toBe("Main menu");
    expect(compatibilityStatusFromObservation("process_started")).toBe("");
  });

  it("creates a bounded title and includes only selected related issues", () => {
    expect(compatibilityIssueTitle("Astro Bot", "PPSA21567")).toBe("[GAME STATUS]: Astro Bot (PPSA21567)");
    const body = compatibilityIssueBody({
      gameTitle: "Astro Bot", titleId: "PPSA21567", gameVersion: "01.000", kytyVersion: "KytyPS5 @ abc", status: "In game",
      details: "Works <locally>", steps: "Launch", expected: "Game starts", os: "Linux", cpu: "CPU", gpu: "GPU",
      memory: "16 GiB", patchConfig: "", relatedIssues: [{ number: 42, title: "Same title", state: "open" }], log: "",
    });
    expect(body).toContain("Works \\<locally\\>");
    expect(body).toContain("https://github.com/KytyPS5/KytyPS5/issues/42");
    expect(body).toContain("No log was included");
  });
});
