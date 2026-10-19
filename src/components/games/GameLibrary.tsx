import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Gamepad2 } from "lucide-react";
import type { Game } from "../../types/studio";
import { getGridColumnCount, getVirtualWindow } from "./virtualWindow";
import { date, formatBytes } from "../../utils/format";
import { loadGameIcon, retainGameIcon } from "../../services/gameArtwork";

export function GameCard({
  game,
  selected,
  onSelect,
  mode,
}: {
  game: Game;
  selected: boolean;
  onSelect: () => void;
  mode: "grid" | "list";
}) {
  const element = useRef<HTMLButtonElement>(null);
  const [icon, setIcon] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    const release = retainGameIcon(game.id);
    const load = () => {
      void loadGameIcon(game.id).then((url) => {
        if (!disposed) setIcon(url);
      });
    };
    const target = element.current;
    if (!target) {
      release();
      return;
    }
    if (typeof IntersectionObserver === "undefined") {
      load();
      return () => {
        disposed = true;
        release();
      };
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer.disconnect();
          load();
        }
      },
      { rootMargin: "180px" },
    );
    observer.observe(target);
    return () => {
      disposed = true;
      observer.disconnect();
      release();
    };
  }, [game.id]);
  return (
    <button
      ref={element}
      data-game-id={game.id}
      className={`game-card ${mode} ${selected ? "selected" : ""}`}
      onClick={onSelect}
    >
      <div className="game-art">
        {icon ? (
          <img loading="lazy" src={icon} alt={`${game.title} icon`} />
        ) : (
          <Gamepad2 />
        )}
      </div>
      <strong>{game.title}</strong>
      <small>
        {mode === "list"
          ? `${game.title_id || "No title ID"} · ${game.game_version ? `v${game.game_version}` : "Version unknown"} · ${formatBytes(game.install_size)} · ${Math.round(game.play_seconds / 60)} min · ${game.build_id ? `Build ${game.build_id}` : "Active build"} · ${game.file_status === "Missing" ? "Missing files" : game.last_played ? `Played ${date(game.last_played)}` : "Not played yet"}`
          : game.file_status === "Missing"
            ? "Missing files"
            : game.last_played
              ? `Played ${date(game.last_played)}`
              : "Not played yet"}
      </small>
    </button>
  );
}
export function HomeGameTile({
  game,
  selected,
  onSelect,
}: {
  game: Game;
  selected: boolean;
  onSelect: () => void;
}) {
  const [icon, setIcon] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    const release = retainGameIcon(game.id);
    void loadGameIcon(game.id).then((url) => {
      if (!disposed) setIcon(url);
    });
    return () => {
      disposed = true;
      release();
    };
  }, [game.id]);
  return (
    <button
      className={`home-game-tile ${selected ? "selected" : ""}`}
      onClick={onSelect}
      aria-pressed={selected}
    >
      <span className="home-game-art">
        {icon ? (
          <img src={icon} alt="" loading="lazy" />
        ) : (
          <Gamepad2 size={25} />
        )}
      </span>
      <strong title={game.title}>{game.title}</strong>
    </button>
  );
}
export function VirtualGameGrid({
  games,
  selectedId,
  onSelect,
  mode,
}: {
  games: Game[];
  selectedId: number | undefined;
  onSelect: (id: number) => void;
  mode: "grid" | "list";
}) {
  const grid = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({
    width: 0,
    height: 0,
    top: 0,
    rowHeight: 274,
    columnGap: 15,
    minimumCardWidth: 160,
  });
  useLayoutEffect(() => {
    const element = grid.current;
    if (!element) return;
    const measure = () => {
      const card = element.querySelector<HTMLElement>(".game-card");
      const styles = getComputedStyle(element);
      const gap = parseFloat(styles.rowGap) || 0;
      const columnGap = parseFloat(styles.columnGap) || 0;
      const minimumCardWidth =
        Number.parseFloat(styles.getPropertyValue("--game-card-min-width")) ||
        160;
      const measured = card
        ? card.getBoundingClientRect().height + gap
        : viewport.rowHeight;
      setViewport((current) => {
        const next = {
          width: element.clientWidth,
          height: element.clientHeight,
          top: element.scrollTop,
          rowHeight: measured > 0 ? measured : current.rowHeight,
          columnGap,
          minimumCardWidth,
        };
        return Object.keys(next).every(
          (key) =>
            next[key as keyof typeof next] ===
            current[key as keyof typeof current],
        )
          ? current
          : next;
      });
    };
    measure();
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(measure);
    observer?.observe(element);
    const card = element.querySelector<HTMLElement>(".game-card");
    if (card) observer?.observe(card);
    if (!observer) window.addEventListener("resize", measure);
    return () => {
      observer?.disconnect();
      if (!observer) window.removeEventListener("resize", measure);
    };
  }, [games.length, mode]);
  useLayoutEffect(() => {
    if (grid.current) grid.current.scrollTop = 0;
    setViewport((current) =>
      current.top === 0 ? current : { ...current, top: 0 },
    );
  }, [games, mode]);
  const columns = getGridColumnCount(
    viewport.width,
    viewport.columnGap,
    viewport.minimumCardWidth,
    mode,
  );
  const rowHeight = viewport.rowHeight;
  const rowGap = parseFloat(
    grid.current ? getComputedStyle(grid.current).rowGap : "15",
  ) || 15;
  const virtual = getVirtualWindow(
    games.length,
    columns,
    viewport.top,
    viewport.height,
    rowHeight,
    rowGap,
  );
  const { totalRows, firstRow, endRow } = virtual;
  const visible = games.slice(virtual.startIndex, virtual.endIndex);
  const gamepadFocus = useRef(false);
  useEffect(() => {
    const move = (event: Event) => {
      const detail = (
        event as CustomEvent<{ direction: string; gameId: number }>
      ).detail;
      const direction = detail?.direction;
      if (!["left", "right", "up", "down"].includes(direction)) return;
      const selectedIndex = games.findIndex(
        (game) => game.id === (detail.gameId || selectedId),
      );
      const index =
        selectedIndex >= 0
          ? selectedIndex
          : games.findIndex((game) => game.id === selectedId);
      const next =
        index < 0
          ? 0
          : index +
            (direction === "left"
              ? -1
              : direction === "right"
                ? 1
                : direction === "up"
                  ? -columns
                  : columns);
      if (next < 0 || next >= games.length) return;
      gamepadFocus.current = true;
      const target = games[next];
      if (grid.current)
        grid.current.scrollTop = Math.max(
          0,
          Math.floor(next / columns) * rowHeight - 2 * rowHeight,
        );
      onSelect(target.id);
    };
    window.addEventListener("studio:gamepad-move", move);
    return () => window.removeEventListener("studio:gamepad-move", move);
  }, [games, selectedId, columns]);
  useEffect(() => {
    if (!gamepadFocus.current || selectedId === undefined) return;
    const target = grid.current?.querySelector<HTMLButtonElement>(
      `[data-game-id="${selectedId}"]`,
    );
    if (target) {
      target.focus({ preventScroll: true });
      gamepadFocus.current = false;
    }
  }, [selectedId, viewport.top, visible]);
  return (
    <div
      ref={grid}
      className={`game-grid ${mode}`}
      onScroll={(event) => {
        const top = event.currentTarget.scrollTop;
        setViewport((current) =>
          Math.floor(top / rowHeight) === Math.floor(current.top / rowHeight)
            ? current
            : { ...current, top },
        );
      }}
      style={{ gridTemplateColumns: `repeat(${columns},minmax(0,1fr))` }}
    >
      {firstRow > 0 && (
        <div
          aria-hidden="true"
          className="virtual-spacer"
          style={{ gridColumn: "1 / -1", height: virtual.topSpacer }}
        />
      )}
      {visible.map((game) => (
        <GameCard
          key={game.id}
          game={game}
          selected={selectedId === game.id}
          onSelect={() => onSelect(game.id)}
          mode={mode}
        />
      ))}
      {endRow < totalRows && (
        <div
          aria-hidden="true"
          className="virtual-spacer"
          style={{ gridColumn: "1 / -1", height: virtual.bottomSpacer }}
        />
      )}
    </div>
  );
}
