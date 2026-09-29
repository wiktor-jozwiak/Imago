import type { FileReport, JobSummary } from "../types/imago";
import { createEntry } from "../utils/files";
import { IDLE_JOB, overallProgress, reduceJobEvents } from "./jobReducer";

function report(id: number): FileReport {
  return {
    id,
    inputPath: `/p/${id}.png`,
    outputPath: `/p/optimized/${id}.webp`,
    status: "done",
    inputFormat: "png",
    outputFormat: "webp",
    inputSize: 1000,
    outputSize: 400,
    inputDimensions: [10, 10],
    outputDimensions: [10, 10],
    reductionPercent: 60,
    timings: { readMs: 1, decodeMs: 1, resizeMs: 0, encodeMs: 5, writeMs: 1, totalMs: 8 },
    decision: null,
    warnings: [],
    error: null,
  };
}

const summary: JobSummary = {
  total: 2,
  done: 2,
  skipped: 0,
  failed: 0,
  cancelled: 0,
  inputBytes: 2000,
  outputBytes: 800,
  elapsedMs: 20,
  wasCancelled: false,
};

describe("job event reducer", () => {
  const entries = {
    1: createEntry(1, { path: "/p/1.png", root: null, size: 1000 }, null),
    2: createEntry(2, { path: "/p/2.png", root: null, size: 1000 }, null),
  };

  it("tracks a job from start to finish", () => {
    let state = reduceJobEvents(entries, IDLE_JOB, [
      { type: "started", total: 2, workers: 2 },
      { type: "fileStarted", id: 1 },
      { type: "fileProgress", id: 1, stage: "encoding", progress: 0.45 },
    ]);
    expect(state.job.running).toBe(true);
    expect(state.entries[1]?.status).toBe("processing");
    expect(state.entries[1]?.stage).toBe("encoding");
    expect(state.entries[2]).toBe(entries[2]);
    expect(overallProgress(Object.values(state.entries), state.job)).toBeCloseTo(0.225);

    state = reduceJobEvents(state.entries, state.job, [
      { type: "fileFinished", report: report(1) },
      { type: "fileFinished", report: report(2) },
      { type: "finished", summary },
    ]);
    expect(state.entries[1]?.status).toBe("done");
    expect(state.entries[1]?.report?.outputSize).toBe(400);
    expect(state.job.finished).toBe(2);
    expect(state.job.running).toBe(false);
    expect(state.job.summary).toEqual(summary);
  });

  it("returns the same objects when nothing changed", () => {
    const state = reduceJobEvents(entries, IDLE_JOB, [{ type: "fileProgress", id: 99, stage: "reading", progress: 0.1 }]);
    expect(state.entries).toBe(entries);
  });

  it("ignores progress for files that are not processing", () => {
    const state = reduceJobEvents(entries, IDLE_JOB, [{ type: "fileProgress", id: 1, stage: "writing", progress: 0.9 }]);
    expect(state.entries[1]?.progress).toBe(0);
  });
});
