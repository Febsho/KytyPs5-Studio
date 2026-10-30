import { Trophy } from "lucide-react";
import type { GameContent } from "../../types/studio";
import { trophyGrade } from "../../utils/trophies";

export function TrophyItem({ trophy }: { trophy: GameContent["trophies"][number] }) {
  const grade = trophyGrade(trophy.grade);
  const masked = trophy.hidden && trophy.unlocked !== true;
  return <article className={`trophy-item trophy-${grade}`}>
    <span className={`trophy-art${trophy.unlocked === false ? " locked" : ""}`}>
      {!masked && trophy.icon ? <img src={trophy.icon} alt="" loading="lazy" /> : <Trophy size={28} aria-hidden="true" />}
    </span>
    <span className="trophy-copy">
      <strong>{masked ? "Hidden trophy" : trophy.name || `Trophy ${trophy.id}`}</strong>
      <span className="trophy-grade"><Trophy size={13} aria-hidden="true" /> {grade === "other" ? trophy.grade || "Trophy" : grade}</span>
      <small>{masked ? "This trophy is hidden." : trophy.detail || `Trophy ${trophy.id}`}</small>
    </span>
    <span className={`trophy-earned-state ${trophy.unlocked === true ? "earned" : ""}`}>{trophy.unlocked === true ? "✓ Earned" : trophy.unlocked === false ? "Not earned" : "Status unavailable"}</span>
  </article>;
}
