import type { JobEvent, JobSummary } from "../types/imago";
import type { FileEntry } from "../utils/files";

export interface JobState {
  id: number | null;
  running: boolean;
  total: number;
  finished: number;
  workers: number;
  startedAt: number | null;
  summary: JobSummary | null;
}

export const IDLE_JOB: JobState = {
  id: null,
  running: false,
  total: 0,
  finished: 0,
  workers: 0,
  startedAt: null,
  summary: null,
};

/**
 * Applies a batch of job events. Pure: returns new objects only for what
 * changed, so React re-renders only the affected rows.
 */
export function reduceJobEvents(
  entries: Record<number, FileEntry>,
  job: JobState,
  events: JobEvent[],
): { entries: Record<number, FileEntry>; job: JobState } {
  let nextEntries = entries;
  let nextJob = job;
  const touch = (id: number, update: (entry: FileEntry) => FileEntry) => {
    const current = nextEntries[id];
    if (!current) return;
    if (nextEntries === entries) nextEntries = { ...entries };
    nextEntries[id] = update(current);
  };

  for (const event of events) {
    switch (event.type) {
      case "started":
        nextJob = { ...nextJob, running: true, total: event.total, workers: event.workers };
        break;
      case "fileStarted":
        touch(event.id, (entry) => ({ ...entry, status: "processing", stage: "reading", progress: 0.02 }));
        break;
      case "fileProgress":
        touch(event.id, (entry) =>
          entry.status === "processing" ? { ...entry, stage: event.stage, progress: event.progress } : entry,
        );
        break;
      case "fileFinished":
        touch(event.report.id, (entry) => ({
          ...entry,
          status: event.report.status,
          stage: null,
          progress: 1,
          report: event.report,
        }));
        nextJob = { ...nextJob, finished: nextJob.finished + 1 };
        break;
      case "finished":
        nextJob = { ...nextJob, running: false, summary: event.summary };
        break;
    }
  }
  return { entries: nextEntries, job: nextJob };
}

/** Overall progress (0..1): finished files plus the partial progress of running ones. */
export function overallProgress(entries: FileEntry[], job: JobState): number {
  if (job.total === 0) return 0;
  let partial = 0;
  for (const entry of entries) {
    if (entry.status === "processing") partial += entry.progress;
  }
  return Math.min(1, (job.finished + partial) / job.total);
}
