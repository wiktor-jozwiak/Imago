/**
 * TypeScript mirrors of the serializable structures of `imago-core` and the
 * Tauri adapter. Field names follow `#[serde(rename_all = "camelCase")]`.
 */

export type ImageFormat = "jpeg" | "png" | "webp" | "avif" | "tiff" | "bmp" | "gif" | "jxl" | "heic";

export type FormatChoice = { mode: "auto" } | { mode: "original" } | { mode: "fixed"; format: ImageFormat };

export type Effort = "fast" | "balanced" | "max";

export interface CompressionSettings {
  quality: number;
  lossless: boolean;
  effort: Effort;
}

export type ResizeMode =
  | { mode: "none" }
  | { mode: "dimensions"; width: number | null; height: number | null; keepAspect: boolean }
  | { mode: "fit"; maxWidth: number | null; maxHeight: number | null }
  | { mode: "percent"; percent: number };

export interface ResizeSettings {
  mode: ResizeMode;
  allowUpscale: boolean;
}

export interface MetadataSettings {
  stripMetadata: boolean;
  keepIcc: boolean;
  preserveTimestamps: boolean;
  autoOrient: boolean;
}

export type OutputLocation =
  | { kind: "subfolder"; name: string }
  | { kind: "directory"; path: string }
  | { kind: "sameFolder" }
  | { kind: "file"; path: string };

export type ConflictPolicy = "rename" | "skip" | "overwrite";
export type WhenLarger = "keepOriginal" | "writeAnyway";
export type AnimationPolicy = "preserve" | "firstFrame";
export type TransparencyPolicy = "preserve" | "flattenWhite";

export interface OutputSettings {
  location: OutputLocation;
  suffix: string;
  preserveStructure: boolean;
  conflict: ConflictPolicy;
  allowOverwriteOriginals: boolean;
  whenLarger: WhenLarger;
  fsync: boolean;
}

export interface Limits {
  maxFileBytes: number;
  maxDimension: number;
  maxPixels: number;
  maxAnimationPixels: number;
  memoryBudgetBytes: number | null;
}

export interface OptimizeSettings {
  format: FormatChoice;
  compression: CompressionSettings;
  resize: ResizeSettings;
  metadata: MetadataSettings;
  output: OutputSettings;
  animation: AnimationPolicy;
  transparency: TransparencyPolicy;
  limits: Limits;
  largeImages: LargeImages;
  workers: number | null;
}

/** Images above the limits: processed row by row (JPEG/PNG) or rejected. */
export type LargeImages = "stream" | "reject";

export interface ScanOptions {
  recursive: boolean;
  includeHidden: boolean;
  excludeDirNames: string[];
}

export type Theme = "system" | "light" | "dark";

export interface AppConfig {
  version: number;
  settings: OptimizeSettings;
  scan: ScanOptions;
  ui: { theme: Theme; language: string | null; showAdvanced: boolean };
  zip: { fileName: string; createAfterJob: boolean };
}

export interface AppInfo {
  name: string;
  fullName: string;
  tagline: string;
  taglinePl: string;
  version: string;
}

export interface FormatCapability {
  format: ImageFormat;
  decode: boolean;
  encode: boolean;
  lossy: boolean;
  lossless: boolean;
  alpha: boolean;
  animation: boolean;
  icc: boolean;
  exif: boolean;
  maxDimension: number | null;
  experimental: boolean;
  unavailableReason: string | null;
  alternative: ImageFormat | null;
  howToEnable: string | null;
}

export interface AppInfoPayload {
  info: AppInfo;
  capabilities: FormatCapability[];
  defaultWorkers: number;
  platform: string;
  logDir: string | null;
}

export interface InputFile {
  path: string;
  root: string | null;
  size: number;
}

export interface ErrorInfo {
  code: string;
  message: string;
  alternative?: string;
  hint?: string;
}

export interface FileInfo {
  path: string;
  size: number;
  format: ImageFormat | null;
  width: number | null;
  height: number | null;
  animated: boolean;
  hasAlpha: boolean;
  error: ErrorInfo | null;
}

export interface ScannedFile {
  input: InputFile;
  info: FileInfo;
}

export interface ScanPayload {
  files: ScannedFile[];
  skipped: number;
}

export interface FileTask {
  id: number;
  input: InputFile;
}

export type FileStatus = "pending" | "processing" | "done" | "skipped" | "cancelled" | "failed";

export type Stage = "reading" | "analyzing" | "decoding" | "transforming" | "encoding" | "writing";

export interface StageTimings {
  readMs: number;
  decodeMs: number;
  resizeMs: number;
  encodeMs: number;
  writeMs: number;
  totalMs: number;
}

export type DecisionReason = "requested" | "smallest" | "keptOriginal";

export interface CandidateSize {
  format: ImageFormat;
  lossless: boolean;
  size: number;
}

export interface Decision {
  format: ImageFormat;
  reason: DecisionReason;
  savedPercent: number;
  keptTransparency: boolean;
  keptAnimation: boolean;
  candidates: CandidateSize[];
}

export interface Warning {
  code: string;
  message: string;
}

export interface FileReport {
  id: number;
  inputPath: string;
  outputPath: string | null;
  status: FileStatus;
  inputFormat: ImageFormat | null;
  outputFormat: ImageFormat | null;
  inputSize: number;
  outputSize: number | null;
  inputDimensions: [number, number] | null;
  outputDimensions: [number, number] | null;
  reductionPercent: number | null;
  timings: StageTimings;
  decision: Decision | null;
  warnings: Warning[];
  error: ErrorInfo | null;
}

export interface JobSummary {
  total: number;
  done: number;
  skipped: number;
  failed: number;
  cancelled: number;
  inputBytes: number;
  outputBytes: number;
  elapsedMs: number;
  wasCancelled: boolean;
}

export type JobEvent =
  | { type: "started"; total: number; workers: number }
  | { type: "fileStarted"; id: number }
  | { type: "fileProgress"; id: number; stage: Stage; progress: number }
  | { type: "fileFinished"; report: FileReport }
  | { type: "finished"; summary: JobSummary };

export interface ArchiveSummary {
  path: string;
  files: number;
  size: number;
}

export interface CommandError {
  code: string;
  message: string;
  alternative?: string;
  hint?: string;
}
