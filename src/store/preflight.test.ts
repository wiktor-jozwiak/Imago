import type { FileInfo } from "../types/imago";
import { defaultSettings } from "../utils/defaults";
import { createEntry } from "../utils/files";
import { preflightSteps } from "./useImagoStore";

function withInfo(id: number, info: Partial<FileInfo>) {
  const full: FileInfo = {
    path: `/p/${id}`,
    size: 1,
    format: "gif",
    width: 10,
    height: 10,
    animated: false,
    hasAlpha: false,
    error: null,
    ...info,
  };
  return createEntry(id, { path: full.path, root: null, size: 1 }, full);
}

describe("pre-flight questions", () => {
  const files = [withInfo(1, { animated: true }), withInfo(2, { hasAlpha: true, format: "png" }), withInfo(3, {})];

  it("asks nothing with default settings", () => {
    expect(preflightSteps(defaultSettings(), files)).toEqual([]);
  });

  it("asks about overwriting", () => {
    const settings = defaultSettings();
    settings.output.conflict = "overwrite";
    expect(preflightSteps(settings, files)).toEqual([{ kind: "overwrite" }]);
  });

  it("asks about animations and transparency for JPEG", () => {
    const settings = defaultSettings();
    settings.format = { mode: "fixed", format: "jpeg" };
    expect(preflightSteps(settings, files)).toEqual([
      { kind: "animation", count: 1, format: "jpeg" },
      { kind: "transparency", count: 1 },
    ]);
  });

  it("does not ask when the user already chose", () => {
    const settings = defaultSettings();
    settings.format = { mode: "fixed", format: "jpeg" };
    settings.animation = "firstFrame";
    settings.transparency = "flattenWhite";
    expect(preflightSteps(settings, files)).toEqual([]);
    settings.format = { mode: "fixed", format: "webp" };
    settings.animation = "preserve";
    expect(preflightSteps(settings, files)).toEqual([]);
  });
});
