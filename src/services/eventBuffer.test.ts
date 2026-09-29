import { createEventBuffer } from "./eventBuffer";

describe("event buffer", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("delivers events in batches", () => {
    const batches: number[][] = [];
    const buffer = createEventBuffer<number>((events) => batches.push(events), { intervalMs: 50 });
    buffer.push(1);
    buffer.push(2);
    expect(batches).toEqual([]);
    vi.advanceTimersByTime(50);
    expect(batches).toEqual([[1, 2]]);
    buffer.push(3);
    vi.advanceTimersByTime(50);
    expect(batches).toEqual([[1, 2], [3]]);
  });

  it("flushes urgent events immediately", () => {
    const batches: string[][] = [];
    const buffer = createEventBuffer<string>((events) => batches.push(events), {
      isUrgent: (event) => event === "finished",
    });
    buffer.push("progress");
    buffer.push("finished");
    expect(batches).toEqual([["progress", "finished"]]);
  });
});
