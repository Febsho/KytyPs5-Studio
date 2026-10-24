import type { Game, OfficialCompatibilityIndex } from "../../types/studio";
import {
  compatibilityStatusLabel,
  officialCompatibilityForGame,
} from "../../utils/compatibility";

export function GameCompatibilityBadge({
  game,
  index,
  className = "",
}: {
  game: Game;
  index: OfficialCompatibilityIndex | null | undefined;
  className?: string;
}) {
  const entry = officialCompatibilityForGame(game, index ?? null);
  if (!entry) return null;

  const status = compatibilityStatusLabel(entry.status);
  const tone = status === "Not Booting"
    ? "blocked"
    : status === "Playable" || status === "In-Game"
      ? "in-game"
      : status === "Main Menu"
        ? "main-menu"
        : status === "Logo"
          ? "logo"
          : "untested";

  return (
    <span
      className={`compatibility-status-badge tone-${tone} ${className}`.trim()}
      title={`Official compatibility result: ${status}`}
      aria-label={`Official compatibility: ${status}`}
    >
      {status}
    </span>
  );
}
