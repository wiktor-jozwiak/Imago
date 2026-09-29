import type { Translate } from "../i18n";
import type { OptimizeSettings, ScanOptions } from "../types/imago";
import { FORMAT_LABELS } from "./defaults";
import { fileName } from "./format";

/** One-line descriptions shown on collapsed settings cards. */

/** Quality presets offered next to the slider. */
export const QUALITY_PRESETS = [
  { key: "presetSmall", quality: 60 },
  { key: "presetBalanced", quality: 82 },
  { key: "presetHigh", quality: 92 },
] as const;

export function formatSummary(settings: OptimizeSettings, t: Translate): string {
  switch (settings.format.mode) {
    case "auto":
      return t("format.modeAuto");
    case "original":
      return t("format.modeOriginal");
    case "fixed":
      return FORMAT_LABELS[settings.format.format];
  }
}

export function qualitySummary(settings: OptimizeSettings, t: Translate): string {
  const compression = settings.compression;
  const parts: string[] = [];
  if (compression.lossless) {
    parts.push(t("settings.lossless"));
  } else {
    const preset = QUALITY_PRESETS.find((item) => item.quality === compression.quality);
    parts.push(preset ? `${compression.quality} · ${t(`quality.${preset.key}`)}` : String(compression.quality));
  }
  if (compression.effort === "max") parts.push(t("settings.maxCompression").toLowerCase());
  return parts.join(" · ");
}

export function resizeSummary(settings: OptimizeSettings, t: Translate): string {
  const mode = settings.resize.mode;
  switch (mode.mode) {
    case "none":
      return t("summaryResize.none");
    case "dimensions":
      if (mode.width && mode.height) return t("summaryResize.box", { w: mode.width, h: mode.height });
      if (mode.width) return t("summaryResize.width", { w: mode.width });
      if (mode.height) return t("summaryResize.height", { h: mode.height });
      return t("summaryResize.none");
    case "fit":
      if (mode.maxWidth && mode.maxHeight) return t("summaryResize.fit", { w: mode.maxWidth, h: mode.maxHeight });
      if (mode.maxWidth) return t("summaryResize.maxWidth", { w: mode.maxWidth });
      if (mode.maxHeight) return t("summaryResize.maxHeight", { h: mode.maxHeight });
      return t("summaryResize.none");
    case "percent":
      return t("summaryResize.percent", { p: mode.percent });
  }
}

export function metadataSummary(settings: OptimizeSettings, t: Translate): string {
  const parts = [
    settings.metadata.stripMetadata ? t("summaryMetadata.strip") : t("summaryMetadata.keep"),
    settings.metadata.keepIcc ? t("summaryMetadata.icc") : t("summaryMetadata.srgb"),
  ];
  if (settings.metadata.preserveTimestamps) parts.push(t("summaryMetadata.dates"));
  return parts.join(" · ");
}

export function outputSummary(settings: OptimizeSettings, t: Translate): string {
  const location = settings.output.location;
  let where: string;
  switch (location.kind) {
    case "subfolder":
      where = t("summaryOutput.subfolder", { name: location.name });
      break;
    case "directory":
      where = location.path ? t("summaryOutput.directory", { name: fileName(location.path) }) : t("output.noFolder");
      break;
    case "sameFolder":
      where = t("summaryOutput.same");
      break;
    case "file":
      where = fileName(location.path);
      break;
  }
  const conflict = t(`summaryConflict.${settings.output.conflict}`);
  const suffix = settings.output.suffix ? ` · „${settings.output.suffix}”` : "";
  return `${where} · ${conflict}${suffix}`;
}

export function advancedSummary(settings: OptimizeSettings, scan: ScanOptions, defaultWorkers: number, t: Translate): string {
  const workers = settings.workers ?? defaultWorkers;
  const parts = [t("summaryAdvanced.workers", { n: workers })];
  parts.push(t("summaryAdvanced.limits", { mp: Math.round(settings.limits.maxPixels / 1_000_000) }));
  if (scan.recursive) parts.push(t("advanced.recursive").toLowerCase());
  return parts.join(" · ");
}
