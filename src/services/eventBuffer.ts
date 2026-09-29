/**
 * Collects high-frequency events and hands them over in batches, so a job
 * with thousands of files triggers a handful of React updates per second
 * instead of one per event.
 */
export interface EventBuffer<T> {
  push(event: T): void;
  flush(): void;
}

export function createEventBuffer<T>(
  deliver: (events: T[]) => void,
  options: { intervalMs?: number; isUrgent?: (event: T) => boolean } = {},
): EventBuffer<T> {
  const intervalMs = options.intervalMs ?? 80;
  let pending: T[] = [];
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
    if (pending.length === 0) return;
    const batch = pending;
    pending = [];
    deliver(batch);
  };

  return {
    push(event: T) {
      pending.push(event);
      if (options.isUrgent?.(event)) {
        flush();
      } else if (timer === null) {
        timer = setTimeout(flush, intervalMs);
      }
    },
    flush,
  };
}
