import { invoke } from '@tauri-apps/api/core';
import { AsyncLruCache } from './AsyncLruCache';
import { AsyncSemaphore } from './AsyncSemaphore';

const gameIconCache = new AsyncLruCache<number, string>(32, url => URL.revokeObjectURL(url));
const gameBackgroundCache = new AsyncLruCache<number, string>(4, url => URL.revokeObjectURL(url));
const iconRequests = new AsyncSemaphore(4);
const backgroundRequests = new AsyncSemaphore(1);
const load = (command: 'read_game_icon' | 'read_game_background', cache: AsyncLruCache<number, string>, requests: AsyncSemaphore, gameId: number) => cache.get(gameId, async () => {
  return requests.run(async () => {
    const bytes = await invoke<ArrayBuffer>(command, { gameId });
    return bytes.byteLength ? URL.createObjectURL(new Blob([bytes], { type: 'image/png' })) : null;
  });
});
export const loadGameIcon = (gameId: number) => load('read_game_icon', gameIconCache, iconRequests, gameId);
export const loadGameBackground = (gameId: number) => load('read_game_background', gameBackgroundCache, backgroundRequests, gameId);
export const retainGameIcon = (gameId: number) => gameIconCache.retain(gameId);
export const retainGameBackground = (gameId: number) => gameBackgroundCache.retain(gameId);
