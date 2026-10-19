import { describe, expect, it, vi } from 'vitest';
import { AsyncLruCache } from './AsyncLruCache';

describe('AsyncLruCache', () => {
  it('shares an in-flight load and promotes cache hits to most recently used', async () => {
    const dispose = vi.fn();
    const cache = new AsyncLruCache<number, string>(2, dispose);
    const loadA = vi.fn(async () => 'a');
    expect(await cache.get(1, loadA)).toBe('a');
    expect(await cache.get(1, loadA)).toBe('a');
    await cache.get(2, async () => 'b');
    await cache.get(1, loadA);
    await cache.get(3, async () => 'c');
    await Promise.resolve();
    expect(loadA).toHaveBeenCalledTimes(1);
    expect(dispose).toHaveBeenCalledWith('b');
  });

  it('disposes resolved values on eviction and clear', async () => {
    const dispose = vi.fn();
    const cache = new AsyncLruCache<number, string>(1, dispose);
    await cache.get(1, async () => 'first');
    await cache.get(2, async () => 'second');
    await Promise.resolve();
    expect(dispose).toHaveBeenCalledWith('first');
    cache.clear();
    await Promise.resolve();
    expect(dispose).toHaveBeenCalledWith('second');
  });

  it('keeps retained resources alive through capacity pressure', async () => {
    const dispose = vi.fn();
    const cache = new AsyncLruCache<number, string>(1, dispose);
    const release = cache.retain(1);
    await cache.get(1, async () => 'visible image');
    await cache.get(2, async () => 'new image');
    await Promise.resolve();
    expect(dispose).not.toHaveBeenCalledWith('visible image');
    release();
    await cache.get(3, async () => 'third image');
    await Promise.resolve();
    expect(dispose).toHaveBeenCalledWith('visible image');
  });

  it('defers clear until the final consumer releases a retained resource', async () => {
    const dispose = vi.fn();
    const cache = new AsyncLruCache<number, string>(1, dispose);
    const release = cache.retain(1);
    await cache.get(1, async () => 'visible image');
    cache.clear();
    await Promise.resolve();
    expect(dispose).not.toHaveBeenCalled();
    release();
    await Promise.resolve();
    expect(dispose).toHaveBeenCalledWith('visible image');
  });
});
