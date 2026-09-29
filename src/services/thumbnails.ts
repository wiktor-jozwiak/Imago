import { renderImage } from "./backend";

/**
 * Lazily rendered thumbnails: at most `MAX_PARALLEL` renders at a time, results
 * cached per path (bounded, oldest evicted and revoked).
 */
const MAX_PARALLEL = 3;
const MAX_CACHED = 400;

type Listener = (url: string | null) => void;

interface Entry {
  url: string | null;
  failed: boolean;
  listeners: Set<Listener>;
}

const cache = new Map<string, Entry>();
const queue: { path: string; size: number }[] = [];
let active = 0;

function key(path: string, size: number): string {
  return `${size}:${path}`;
}

function pump(): void {
  while (active < MAX_PARALLEL && queue.length > 0) {
    const next = queue.shift();
    if (!next) break;
    const entry = cache.get(key(next.path, next.size));
    if (!entry || entry.listeners.size === 0) {
      // Nobody is waiting any more (row scrolled away): drop it.
      cache.delete(key(next.path, next.size));
      continue;
    }
    active += 1;
    renderImage(next.path, next.size)
      .then((url) => {
        entry.url = url;
        entry.listeners.forEach((listener) => listener(url));
        evict();
      })
      .catch(() => {
        entry.failed = true;
        entry.listeners.forEach((listener) => listener(null));
      })
      .finally(() => {
        active -= 1;
        pump();
      });
  }
}

function evict(): void {
  if (cache.size <= MAX_CACHED) return;
  for (const [cacheKey, entry] of cache) {
    if (cache.size <= MAX_CACHED) break;
    if (entry.listeners.size === 0 && entry.url) {
      URL.revokeObjectURL(entry.url);
      cache.delete(cacheKey);
    }
  }
}

/** Subscribes to a thumbnail; the listener is called once it is ready. */
export function subscribeThumbnail(path: string, size: number, listener: Listener): () => void {
  const cacheKey = key(path, size);
  let entry = cache.get(cacheKey);
  if (!entry) {
    entry = { url: null, failed: false, listeners: new Set() };
    cache.set(cacheKey, entry);
    queue.push({ path, size });
  }
  entry.listeners.add(listener);
  if (entry.url || entry.failed) listener(entry.url);
  pump();
  const current = entry;
  return () => {
    current.listeners.delete(listener);
  };
}

/** Forgets a cached image (e.g. after the output file was rewritten). */
export function invalidateThumbnail(path: string, size: number): void {
  const cacheKey = key(path, size);
  const entry = cache.get(cacheKey);
  if (entry?.url) URL.revokeObjectURL(entry.url);
  cache.delete(cacheKey);
}
