/** Human-readable size with binary units ("1.2 MB"). */
export function formatBytes(bytes: number | null | undefined, locale = "en"): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes)) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = Math.abs(bytes);
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const sign = bytes < 0 ? "-" : "";
  if (unit === 0) return `${sign}${value} B`;
  const digits = value >= 100 ? 0 : 1;
  return `${sign}${value.toLocaleString(locale, { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${units[unit]}`;
}

/** Size change as shown in badges: "−71%" or "+12%". */
export function formatReduction(percent: number | null | undefined): string {
  if (percent === null || percent === undefined || !Number.isFinite(percent)) return "—";
  const rounded = Math.round(percent);
  if (rounded === 0) return "0%";
  return rounded > 0 ? `−${rounded}%` : `+${Math.abs(rounded)}%`;
}

/** Duration: "850 ms", "3.2 s", "1 min 5 s". */
export function formatDuration(milliseconds: number | null | undefined, locale = "en"): string {
  if (milliseconds === null || milliseconds === undefined || !Number.isFinite(milliseconds)) return "—";
  if (milliseconds < 1000) return `${Math.max(0, Math.round(milliseconds))} ms`;
  const seconds = milliseconds / 1000;
  if (seconds < 60) {
    return `${seconds.toLocaleString(locale, { maximumFractionDigits: 1 })} s`;
  }
  const minutes = Math.floor(seconds / 60);
  return `${minutes} min ${Math.round(seconds - minutes * 60)} s`;
}

/** "1920 × 1080" or "—". */
export function formatDimensions(width: number | null | undefined, height: number | null | undefined): string {
  if (!width || !height) return "—";
  return `${width} × ${height}`;
}

/** File name part of a path (both separators). */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Folder part of a path. */
export function parentDir(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index > 0 ? path.slice(0, index) : "";
}
