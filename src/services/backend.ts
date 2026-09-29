import { Channel, invoke, isTauri } from "@tauri-apps/api/core";

import type {
  AppConfig,
  AppInfoPayload,
  ArchiveSummary,
  CommandError,
  FileTask,
  InputFile,
  JobEvent,
  OptimizeSettings,
  ScanOptions,
  ScanPayload,
} from "../types/imago";
import { defaultConfig, fallbackCapabilities } from "../utils/defaults";

/**
 * The only module that talks to the Rust side. Outside Tauri (plain browser,
 * unit tests) every call degrades to a harmless default, so the UI can still
 * be rendered.
 */

export function runningInTauri(): boolean {
  return isTauri();
}

/** Normalizes anything thrown by `invoke` into a `CommandError`. */
export function toCommandError(error: unknown): CommandError {
  if (error && typeof error === "object" && "code" in error && "message" in error) {
    return error as CommandError;
  }
  return { code: "internal_error", message: String(error) };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toCommandError(error);
  }
}

export async function getAppInfo(): Promise<AppInfoPayload> {
  if (!runningInTauri()) {
    return {
      info: {
        name: "Imago",
        fullName: "Imago Image Optimizer",
        tagline: "Fast. Local. Smaller images.",
        taglinePl: "Szybsze obrazy. Mniejsze pliki. Zero chmury.",
        version: "dev",
      },
      capabilities: fallbackCapabilities(),
      defaultWorkers: Math.max(1, (navigator.hardwareConcurrency || 2) - 1),
      platform: "browser",
      logDir: null,
    };
  }
  return call<AppInfoPayload>("app_info");
}

export async function loadConfig(): Promise<{ config: AppConfig; warning: string | null }> {
  if (!runningInTauri()) return { config: defaultConfig(), warning: null };
  return call("load_config");
}

export async function saveConfig(config: AppConfig): Promise<void> {
  if (!runningInTauri()) return;
  await call("save_config", { config });
}

export async function pickFiles(title: string): Promise<string[]> {
  if (!runningInTauri()) return [];
  return call<string[]>("pick_files", { title });
}

export async function pickFolder(title: string): Promise<string | null> {
  if (!runningInTauri()) return null;
  return call<string | null>("pick_folder", { title });
}

export async function scanPaths(paths: string[], scan: ScanOptions, settings: OptimizeSettings): Promise<ScanPayload> {
  if (!runningInTauri()) return { files: [], skipped: 0 };
  return call<ScanPayload>("scan_paths", { paths, scan, settings });
}

/** Starts a job; `onEvent` receives every progress event in order. */
export async function startJob(
  files: FileTask[],
  settings: OptimizeSettings,
  onEvent: (event: JobEvent) => void,
): Promise<number> {
  const channel = new Channel<JobEvent>();
  channel.onmessage = onEvent;
  return call<number>("start_job", { files, settings, onEvent: channel });
}

export async function cancelJob(jobId: number): Promise<void> {
  await call("cancel_job", { jobId });
}

export async function skipFile(jobId: number, fileId: number): Promise<void> {
  await call("skip_file", { jobId, fileId });
}

/** Renders a preview; resolves to an object URL the caller must revoke. */
export async function renderImage(path: string, maxSide: number): Promise<string> {
  if (!runningInTauri()) throw toCommandError("preview is only available in the desktop app");
  const bytes = await call<ArrayBuffer>("render_image", { path, maxSide });
  return URL.createObjectURL(new Blob([bytes]));
}

export async function revealPath(path: string): Promise<void> {
  await call("reveal_path", { path });
}

export async function openFolder(path: string): Promise<void> {
  await call("open_folder", { path });
}

export async function exportFile(path: string, title: string): Promise<string | null> {
  return call<string | null>("export_file", { path, title });
}

export async function createZipArchive(
  items: { input: InputFile; output: string }[],
  settings: OptimizeSettings,
  title: string,
): Promise<ArchiveSummary | null> {
  return call<ArchiveSummary | null>("create_zip_archive", { items, settings, title });
}

/** Subscribes to native drag & drop over the window. Returns an unsubscribe function. */
export async function onFileDrop(handlers: {
  over: (active: boolean) => void;
  drop: (paths: string[]) => void;
}): Promise<() => void> {
  if (!runningInTauri()) return () => undefined;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter" || payload.type === "over") {
      handlers.over(true);
    } else if (payload.type === "leave") {
      handlers.over(false);
    } else if (payload.type === "drop") {
      handlers.over(false);
      handlers.drop(payload.paths);
    }
  });
}
