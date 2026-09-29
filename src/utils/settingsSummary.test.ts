import { translator } from "../i18n";
import { defaultConfig, defaultSettings } from "./defaults";
import {
  advancedSummary,
  formatSummary,
  metadataSummary,
  outputSummary,
  qualitySummary,
  resizeSummary,
} from "./settingsSummary";

const t = translator("pl");

describe("settings summaries", () => {
  it("describes the output format", () => {
    const settings = defaultSettings();
    expect(formatSummary(settings, t)).toBe("Automatycznie");
    settings.format = { mode: "original" };
    expect(formatSummary(settings, t)).toBe("Zachowaj format");
    settings.format = { mode: "fixed", format: "jxl" };
    expect(formatSummary(settings, t)).toBe("JPEG XL");
  });

  it("describes quality and names presets", () => {
    const settings = defaultSettings();
    expect(qualitySummary(settings, t)).toBe("82 · Zrównoważona");
    settings.compression.quality = 70;
    settings.compression.effort = "max";
    expect(qualitySummary(settings, t)).toBe("70 · maksymalna kompresja");
    settings.compression.lossless = true;
    expect(qualitySummary(settings, t)).toBe("Tryb bezstratny · maksymalna kompresja");
  });

  it("describes resizing", () => {
    const settings = defaultSettings();
    expect(resizeSummary(settings, t)).toBe("Oryginalne wymiary");
    settings.resize.mode = { mode: "fit", maxWidth: 2400, maxHeight: 1600 };
    expect(resizeSummary(settings, t)).toBe("Maks. 2400 × 1600 px");
    settings.resize.mode = { mode: "dimensions", width: 800, height: null, keepAspect: true };
    expect(resizeSummary(settings, t)).toBe("Szerokość 800 px");
    settings.resize.mode = { mode: "percent", percent: 50 };
    expect(resizeSummary(settings, t)).toBe("50% oryginału");
  });

  it("describes metadata handling", () => {
    const settings = defaultSettings();
    expect(metadataSummary(settings, t)).toBe("Bez EXIF · profil ICC");
    settings.metadata.stripMetadata = false;
    settings.metadata.keepIcc = false;
    settings.metadata.preserveTimestamps = true;
    expect(metadataSummary(settings, t)).toBe("EXIF zachowany · kolory w sRGB · daty zachowane");
  });

  it("describes the output location and conflicts", () => {
    const settings = defaultSettings();
    expect(outputSummary(settings, t)).toBe("Podfolder „optimized” · nowa nazwa przy konflikcie");
    settings.output.location = { kind: "directory", path: "C:\\Users\\ja\\Wyniki" };
    settings.output.conflict = "skip";
    settings.output.suffix = "-min";
    expect(outputSummary(settings, t)).toBe("Folder „Wyniki” · pomijanie istniejących · „-min”");
  });

  it("describes advanced settings", () => {
    const config = defaultConfig();
    expect(advancedSummary(config.settings, config.scan, 7, t)).toBe(
      "Wątki: 7 · limit 150 MP · dodawaj obrazy z podfolderów",
    );
  });
});
