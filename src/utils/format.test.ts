import { fileName, formatBytes, formatDimensions, formatDuration, formatReduction, parentDir } from "./format";

describe("format helpers", () => {
  it("formats sizes with binary units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(5.5 * 1024 * 1024)).toBe("5.5 MB");
    expect(formatBytes(300 * 1024 * 1024)).toBe("300 MB");
    expect(formatBytes(null)).toBe("—");
  });

  it("shows reductions with a sign", () => {
    expect(formatReduction(71.4)).toBe("−71%");
    expect(formatReduction(-12)).toBe("+12%");
    expect(formatReduction(0.2)).toBe("0%");
    expect(formatReduction(null)).toBe("—");
  });

  it("formats durations", () => {
    expect(formatDuration(850)).toBe("850 ms");
    expect(formatDuration(3200)).toBe("3.2 s");
    expect(formatDuration(65_000)).toBe("1 min 5 s");
  });

  it("formats dimensions and paths", () => {
    expect(formatDimensions(1920, 1080)).toBe("1920 × 1080");
    expect(formatDimensions(null, 5)).toBe("—");
    expect(fileName("C:\\zdjęcia\\żółw.png")).toBe("żółw.png");
    expect(fileName("/home/a/b.jpg")).toBe("b.jpg");
    expect(parentDir("/home/a/b.jpg")).toBe("/home/a");
    expect(parentDir("C:\\x\\y.png")).toBe("C:\\x");
  });
});
