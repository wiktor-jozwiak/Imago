import type { FileInfo, FileReport, FileStatus, InputFile, Stage } from "../types/imago";
import { fileName } from "./format";

/** One row of the file list. */
export interface FileEntry {
  id: number;
  input: InputFile;
  name: string;
  info: FileInfo | null;
  status: FileStatus;
  stage: Stage | null;
  progress: number;
  report: FileReport | null;
}

export type StatusFilter = "all" | FileStatus;
export type SortKey = "added" | "name" | "size" | "saved" | "status";
export type SortDirection = "asc" | "desc";

export function createEntry(id: number, input: InputFile, info: FileInfo | null): FileEntry {
  return {
    id,
    input,
    name: fileName(input.path),
    info,
    status: "pending",
    stage: null,
    progress: 0,
    report: null,
  };
}

const STATUS_ORDER: Record<FileStatus, number> = {
  processing: 0,
  failed: 1,
  pending: 2,
  cancelled: 3,
  skipped: 4,
  done: 5,
};

function savedBytes(entry: FileEntry): number {
  const report = entry.report;
  if (!report || report.outputSize === null) return Number.NEGATIVE_INFINITY;
  return report.inputSize - report.outputSize;
}

function compare(a: FileEntry, b: FileEntry, key: SortKey): number {
  switch (key) {
    case "added":
      return a.id - b.id;
    case "name":
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) || a.id - b.id;
    case "size":
      return a.input.size - b.input.size || a.id - b.id;
    case "saved":
      return savedBytes(a) - savedBytes(b) || a.id - b.id;
    case "status":
      return STATUS_ORDER[a.status] - STATUS_ORDER[b.status] || a.id - b.id;
  }
}

/** Filters and sorts entries for display. */
export function visibleEntries(
  entries: FileEntry[],
  filter: StatusFilter,
  sort: SortKey,
  direction: SortDirection,
): FileEntry[] {
  const filtered = filter === "all" ? entries.slice() : entries.filter((entry) => entry.status === filter);
  filtered.sort((a, b) => (direction === "asc" ? compare(a, b, sort) : compare(b, a, sort)));
  return filtered;
}

/** Counts per status, for the filter chips. */
export function statusCounts(entries: FileEntry[]): Record<StatusFilter, number> {
  const counts: Record<StatusFilter, number> = {
    all: entries.length,
    pending: 0,
    processing: 0,
    done: 0,
    skipped: 0,
    cancelled: 0,
    failed: 0,
  };
  for (const entry of entries) counts[entry.status] += 1;
  return counts;
}

/** Files that "Start" should process: everything not done yet, or all when everything is done. */
export function filesToProcess(entries: FileEntry[]): FileEntry[] {
  const unfinished = entries.filter((entry) => entry.status !== "done" && entry.status !== "processing");
  return unfinished.length > 0 ? unfinished : entries;
}
