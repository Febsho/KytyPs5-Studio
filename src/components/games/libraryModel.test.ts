import { describe, expect, it } from 'vitest';
import { filterAndSortGames, type GameLibraryItem } from './libraryModel';

const games: GameLibraryItem[] = [
  { id: 1, title: 'Beta', path: '/games/beta', title_id: 'PPSA00002', favorite: true, last_played: 20, play_seconds: 120, install_size: 200 },
  { id: 2, title: 'Alpha', path: '/games/alpha', title_id: 'PPSA00001', favorite: false, last_played: null, play_seconds: 0, install_size: null },
  { id: 3, title: 'Gamma', path: '/archive/third', title_id: 'PPSA00003', favorite: true, last_played: 10, play_seconds: 600, install_size: 500 },
];

describe('game library filtering and sorting', () => {
  it('searches titles, title IDs, and paths without changing the source list', () => {
    expect(filterAndSortGames(games, 'ppsa00003', 'all', 'title').map(game => game.id)).toEqual([3]);
    expect(filterAndSortGames(games, '/archive', 'all', 'title').map(game => game.id)).toEqual([3]);
    expect(games.map(game => game.id)).toEqual([1, 2, 3]);
  });

  it('filters favorites and recent games and sorts by the selected field', () => {
    expect(filterAndSortGames(games, '', 'favorites', 'title').map(game => game.id)).toEqual([1, 3]);
    expect(filterAndSortGames(games, '', 'recent', 'recent').map(game => game.id)).toEqual([1, 3]);
    expect(filterAndSortGames(games, '', 'all', 'playtime').map(game => game.id)).toEqual([3, 1, 2]);
    expect(filterAndSortGames(games, '', 'all', 'size').map(game => game.id)).toEqual([3, 1, 2]);
  });
});
