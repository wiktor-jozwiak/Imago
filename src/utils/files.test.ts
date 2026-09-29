import type { FileReport, FileStatus } from "../types/imago";
import { type FileEntry, createEntry, filesToProcess, statusCounts, visibleEntries } from "./files";

function entry(id: number, name: string, size: number, status: FileStatus, saved?: number): FileEntry {
  const base = createEntry(id, { path: `/p/${name}`, root: null, size }, null);
  const report =
    saved === undefined
      ? null
      : ({ id, inputSize: size, outputSize: size - saved, status } as unknown as FileReport);
  return { ...base, status, report };
}

const entries = [
  entry(1, "b.png", 300, "done", 100),
  entry(2, "a.png", 100, "failed"),
  entry(3, "c10.png", 200, "done", 150),
  entry(4, "c2.png", 50, "pending"),
];

describe("file list helpers", () => {
  it("filters by status", () => {
    expect(visibleEntries(entries, "done", "added", "asc").map((e) => e.id)).toEqual([1, 3]);
    expect(visibleEntries(entries, "all", "added", "asc")).toHaveLength(4);
  });

  it("sorts by name naturally, size, savings and status", () => {
    expect(visibleEntries(entries, "all", "name", "asc").map((e) => e.name)).toEqual(["a.png", "b.png", "c2.png", "c10.png"]);
    expect(visibleEntries(entries, "all", "size", "desc").map((e) => e.id)).toEqual([1, 3, 2, 4]);
    expect(visibleEntries(entries, "all", "saved", "desc").map((e) => e.id).slice(0, 2)).toEqual([3, 1]);
    expect(visibleEntries(entries, "all", "status", "asc")[0]?.status).toBe("failed");
  });

  it("counts statuses", () => {
    const counts = statusCounts(entries);
    expect(counts.all).toBe(4);
    expect(counts.done).toBe(2);
    expect(counts.failed).toBe(1);
  });

  it("processes unfinished files first, everything when all are done", () => {
    expect(filesToProcess(entries).map((e) => e.id)).toEqual([2, 4]);
    const allDone = [entry(1, "x", 1, "done", 0), entry(2, "y", 1, "done", 0)];
    expect(filesToProcess(allDone)).toHaveLength(2);
  });
});
