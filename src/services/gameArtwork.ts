import { invoke } from '@tauri-apps/api/core';
import { AsyncLruCache } from './AsyncLruCache';
import { AsyncSemaphore } from './AsyncSemaphore';

const gameIconCache = new AsyncLruCache<number, string>(32, url => URL.revokeObjectURL(url));
const gameBackgroundCache = new AsyncLruCache<number, string>(4, url => URL.revokeObjectURL(url));
const artworkRequests = new AsyncSemaphore(2);
const load = (command: 'read_game_icon' | 'read_game_background', cache: AsyncLruCache<number, string>, gameId: number) => cache.get(gameId, async () => {
  return artworkRequests.run(async () => {
    const bytes = await invoke<number[] | null>(command, { gameId });
    return bytes ? URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'image/png' })) : null;
  });
});
export const loadGameIcon = (gameId: number) => load('read_game_icon', gameIconCache, gameId);
export const loadGameBackground = (gameId: number) => load('read_game_background', gameBackgroundCache, gameId);
export const retainGameIcon = (gameId: number) => gameIconCache.retain(gameId);
export const retainGameBackground = (gameId: number) => gameBackgroundCache.retain(gameId);
