export type GameLibraryItem = {
  id: number;
  title: string;
  path: string;
  title_id: string | null;
  favorite: boolean;
  last_played: number | null;
  play_seconds: number;
  install_size: number | null;
};

export type LibraryFilter = 'all' | 'favorites' | 'recent';
export type LibrarySort = 'title' | 'recent' | 'playtime' | 'size';

const genericExecutableTitles = /^(?:app(?:0)?|eboot(?:\.bin)?|unknown(?: game)?|game)$/i;

export function isPubliclyRecognizedGame(game: Pick<GameLibraryItem, 'title' | 'title_id'>): boolean {
  return Boolean(game.title_id?.trim()) && !genericExecutableTitles.test(game.title.trim());
}

export function filterAndSortGames<T extends GameLibraryItem>(
  games: readonly T[],
  query: string,
  filter: LibraryFilter,
  sort: LibrarySort,
): T[] {
  const term = query.trim().toLocaleLowerCase();
  const result = games.filter(game =>
    (filter !== 'favorites' || game.favorite)
    && (filter !== 'recent' || game.last_played !== null)
    && (!term || `${game.title} ${game.path} ${game.title_id || ''}`.toLocaleLowerCase().includes(term)),
  );
  return result.sort((a, b) => {
    if (sort === 'recent') return (b.last_played ?? 0) - (a.last_played ?? 0) || a.title.localeCompare(b.title);
    if (sort === 'playtime') return b.play_seconds - a.play_seconds || a.title.localeCompare(b.title);
    if (sort === 'size') return (b.install_size ?? -1) - (a.install_size ?? -1) || a.title.localeCompare(b.title);
    return a.title.localeCompare(b.title);
  });
}
