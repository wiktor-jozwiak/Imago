import type { AppConfig, FormatCapability, ImageFormat, OptimizeSettings } from "../types/imago";

/** Mirrors `OptimizeSettings::default()` in imago-core (used before the backend answers and in tests). */
export function defaultSettings(): OptimizeSettings {
  return {
    format: { mode: "auto" },
    compression: { quality: 82, lossless: false, effort: "balanced" },
    resize: { mode: { mode: "none" }, allowUpscale: false },
    metadata: { stripMetadata: true, keepIcc: true, preserveTimestamps: false, autoOrient: true },
    output: {
      location: { kind: "subfolder", name: "optimized" },
      suffix: "",
      preserveStructure: true,
      conflict: "rename",
      allowOverwriteOriginals: false,
      whenLarger: "keepOriginal",
      fsync: true,
    },
    animation: "preserve",
    transparency: "preserve",
    limits: {
      maxFileBytes: 512 * 1024 * 1024,
      maxDimension: 32_768,
      maxPixels: 150_000_000,
      maxAnimationPixels: 1_000_000_000,
      memoryBudgetBytes: null,
    },
    workers: null,
  };
}

/** Mirrors `AppConfig::default()`. */
export function defaultConfig(): AppConfig {
  return {
    version: 1,
    settings: defaultSettings(),
    scan: { recursive: true, includeHidden: false, excludeDirNames: [] },
    ui: { theme: "system", language: null, showAdvanced: false },
    zip: { fileName: "imago-optimized.zip", createAfterJob: false },
  };
}

/** Output formats offered in the format picker, in display order. */
export const OUTPUT_FORMATS: ImageFormat[] = ["jpeg", "png", "webp", "avif", "tiff", "jxl"];

/** Capabilities used when the backend is not available (browser preview, tests). */
export function fallbackCapabilities(): FormatCapability[] {
  const base = {
    decode: true,
    encode: true,
    lossy: true,
    lossless: false,
    alpha: true,
    animation: false,
    icc: true,
    exif: true,
    maxDimension: null,
    experimental: false,
    unavailableReason: null,
    alternative: null,
    howToEnable: null,
  };
  return [
    { ...base, format: "jpeg", alpha: false, maxDimension: 65500 },
    { ...base, format: "png", lossless: true },
    { ...base, format: "webp", lossless: true, animation: true, maxDimension: 16383 },
    { ...base, format: "avif", icc: false },
    { ...base, format: "tiff", lossy: false, lossless: true, exif: false },
    { ...base, format: "bmp", encode: false, lossy: false },
    { ...base, format: "gif", animation: true, icc: false, exif: false },
    { ...base, format: "jxl", lossy: false, lossless: true, icc: false, exif: false, experimental: true },
  ];
}

export const FORMAT_LABELS: Record<ImageFormat, string> = {
  jpeg: "JPEG",
  png: "PNG",
  webp: "WebP",
  avif: "AVIF",
  tiff: "TIFF",
  bmp: "BMP",
  gif: "GIF",
  jxl: "JPEG XL",
};
