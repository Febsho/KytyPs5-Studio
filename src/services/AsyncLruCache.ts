export class AsyncLruCache<K, V> {
  private readonly values = new Map<K, Promise<V | null>>();
  private readonly retained = new Map<K, number>();
  private readonly clearOnRelease = new Set<K>();

  constructor(
    private readonly limit: number,
    private readonly dispose: (value: V) => void,
  ) {
    if (!Number.isInteger(limit) || limit < 1) throw new RangeError('Cache limit must be positive');
  }

  get(key: K, load: () => Promise<V | null>): Promise<V | null> {
    const cached = this.values.get(key);
    if (cached) {
      this.values.delete(key);
      this.values.set(key, cached);
      return cached;
    }

    const pending = Promise.resolve().then(load).catch(() => null);
    this.values.set(key, pending);
    this.trim();
    return pending;
  }

  retain(key: K): () => void {
    this.retained.set(key, (this.retained.get(key) ?? 0) + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      const count = (this.retained.get(key) ?? 1) - 1;
      if (count > 0) this.retained.set(key, count);
      else {
        this.retained.delete(key);
        if (this.clearOnRelease.delete(key)) this.evict(key);
      }
      this.trim();
    };
  }

  clear(): void {
    for (const key of [...this.values.keys()]) {
      if (this.retained.has(key)) this.clearOnRelease.add(key);
      else this.evict(key);
    }
  }

  private trim(): void {
    while (this.values.size > this.limit) {
      const oldestKey = [...this.values.keys()].find(
        (key) => !this.retained.has(key),
      );
      if (oldestKey === undefined) return;
      this.evict(oldestKey);
    }
  }

  private evict(key: K): void {
    const pending = this.values.get(key);
    if (pending === undefined) return;
    this.values.delete(key);
    pending.then((value) => {
      if (value !== null) this.dispose(value);
    });
  }
}
