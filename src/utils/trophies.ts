import type { GameContent } from "../types/studio";

export const trophyGrades = ["platinum", "gold", "silver", "bronze"] as const;
export type TrophyGrade = (typeof trophyGrades)[number] | "other";

export function trophyGrade(grade?: string | null): TrophyGrade {
  const value = (grade || "").toLowerCase();
  if (value.includes("platinum") || value === "p" || value === "4") return "platinum";
  if (value.includes("gold") || value === "g" || value === "3") return "gold";
  if (value.includes("silver") || value === "s" || value === "2") return "silver";
  if (value.includes("bronze") || value === "b" || value === "1") return "bronze";
  return "other";
}

export function trophySummary(content: GameContent) {
  const counts = Object.fromEntries([...trophyGrades, "other"].map((grade) => [grade, 0])) as Record<TrophyGrade, number>;
  const earned = Object.fromEntries([...trophyGrades, "other"].map((grade) => [grade, 0])) as Record<TrophyGrade, number>;
  for (const trophy of content.trophies) {
    const grade = trophyGrade(trophy.grade);
    counts[grade] += 1;
    if (trophy.unlocked === true) earned[grade] += 1;
  }
  return {
    total: content.trophies.length,
    earned: content.trophy_unlock_state === "available"
      ? content.trophies.filter((trophy) => trophy.unlocked === true).length
      : null,
    counts,
    earnedByGrade: earned,
  };
}
