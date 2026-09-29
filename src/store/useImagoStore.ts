import { create } from "zustand";

import { type Language, type MessageKey, resolveLanguage } from "../i18n";
import * as backend from "../services/backend";
import { createEventBuffer } from "../services/eventBuffer";
import type {
  AppConfig,
  AppInfoPayload,
  CommandError,
  FileTask,
  ImageFormat,
  JobEvent,
  OptimizeSettings,
  Theme,
} from "../types/imago";
import { defaultConfig } from "../utils/defaults";
import {
  type FileEntry,
  type SortDirection,
  type SortKey,
  type StatusFilter,
  createEntry,
  filesToProcess,
} from "../utils/files";
import { fileName, formatBytes, parentDir } from "../utils/format";
import { IDLE_JOB, type JobState, reduceJobEvents } from "./jobReducer";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  key: MessageKey;
  values?: Record<string, string | number>;
  detail?: string;
}

/** A run waiting for the user's answers in pre-flight dialogs. */
export interface PendingRun {
  ids: number[];
  settings: OptimizeSettings;
  steps: PreflightStep[];
}

export type PreflightStep =
  | { kind: "overwrite" }
  | { kind: "animation"; count: number; format: ImageFormat }
  | { kind: "transparency"; count: number };

export type Dialog =
  | { kind: "preflight"; run: PendingRun }
  | { kind: "preview"; id: number }
  | { kind: "shortcuts" }
  | null;

export interface ImagoState {
  ready: boolean;
  appInfo: AppInfoPayload | null;
  config: AppConfig;
  language: Language;
  entries: Record<number, FileEntry>;
  order: number[];
  nextId: number;
  filter: StatusFilter;
  sort: SortKey;
  direction: SortDirection;
  scanning: boolean;
  dragActive: boolean;
  job: JobState;
  dialog: Dialog;
  toasts: Toast[];

  init(): Promise<void>;
  updateConfig(update: (config: AppConfig) => AppConfig): void;
  updateSettings(update: (settings: OptimizeSettings) => OptimizeSettings): void;
  setLanguage(language: Language): void;
  setTheme(theme: Theme): void;
  setDragActive(active: boolean): void;
  addPaths(paths: string[]): Promise<void>;
  addFilesFromDialog(title: string): Promise<void>;
  addFolderFromDialog(title: string): Promise<void>;
  chooseOutputFolder(title: string): Promise<void>;
  removeEntry(id: number): void;
  clearAll(): void;
  clearFinished(): void;
  setFilter(filter: StatusFilter): void;
  setSort(sort: SortKey): void;
  toggleDirection(): void;
  start(ids?: number[]): void;
  answerPreflight(choice: "continue" | "alternative" | "cancel"): void;
  cancelJob(): Promise<void>;
  skipFile(id: number): Promise<void>;
  retryFailed(): void;
  applyEvents(events: JobEvent[]): void;
  openDialog(dialog: Dialog): void;
  closeDialog(): void;
  toast(kind: Toast["kind"], key: MessageKey, values?: Toast["values"], detail?: string): void;
  dismissToast(id: number): void;
  reportError(error: unknown): void;
  createZip(title: string): Promise<void>;
  exportEntry(id: number, title: string): Promise<void>;
  revealEntry(id: number): Promise<void>;
  openOutputFolder(): Promise<void>;
  openLogs(): Promise<void>;
}

let toastCounter = 0;
let saveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleSave(config: AppConfig, onError: (error: unknown) => void): void {
  if (saveTimer !== null) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    backend.saveConfig(config).catch(onError);
  }, 400);
}

function supportsAnimation(format: ImageFormat): boolean {
  return format === "webp" || format === "gif";
}

/** Checks which questions must be asked before a run. */
export function preflightSteps(settings: OptimizeSettings, entries: FileEntry[]): PreflightStep[] {
  const steps: PreflightStep[] = [];
  if (settings.output.conflict === "overwrite") steps.push({ kind: "overwrite" });
  const format = settings.format.mode === "fixed" ? settings.format.format : null;
  if (format && !supportsAnimation(format) && settings.animation === "preserve") {
    const count = entries.filter((entry) => entry.info?.animated).length;
    if (count > 0) steps.push({ kind: "animation", count, format });
  }
  if (format === "jpeg" && settings.transparency === "preserve") {
    const count = entries.filter((entry) => entry.info?.hasAlpha && !entry.info.animated).length;
    if (count > 0) steps.push({ kind: "transparency", count });
  }
  return steps;
}

export const useImagoStore = create<ImagoState>()((set, get) => ({
  ready: false,
  appInfo: null,
  config: defaultConfig(),
  language: resolveLanguage(null, typeof navigator === "undefined" ? [] : navigator.languages ?? []),
  entries: {},
  order: [],
  nextId: 1,
  filter: "all",
  sort: "added",
  direction: "asc",
  scanning: false,
  dragActive: false,
  job: IDLE_JOB,
  dialog: null,
  toasts: [],

  async init() {
    try {
      const [appInfo, loaded] = await Promise.all([backend.getAppInfo(), backend.loadConfig()]);
      const language = resolveLanguage(loaded.config.ui.language, navigator.languages ?? []);
      set({ appInfo, config: loaded.config, language, ready: true });
      if (loaded.warning) get().toast("error", "toast.configReset", undefined, loaded.warning);
    } catch (error) {
      set({ ready: true });
      get().reportError(error);
    }
  },

  updateConfig(update) {
    const config = update(get().config);
    set({ config });
    scheduleSave(config, (error) => get().reportError(error));
  },

  updateSettings(update) {
    get().updateConfig((config) => ({ ...config, settings: update(config.settings) }));
  },

  setLanguage(language) {
    set({ language });
    get().updateConfig((config) => ({ ...config, ui: { ...config.ui, language } }));
  },

  setTheme(theme) {
    get().updateConfig((config) => ({ ...config, ui: { ...config.ui, theme } }));
  },

  setDragActive(active) {
    if (get().dragActive !== active) set({ dragActive: active });
  },

  async addPaths(paths) {
    if (paths.length === 0) return;
    const { config } = get();
    set({ scanning: true });
    try {
      const result = await backend.scanPaths(paths, config.scan, config.settings);
      const state = get();
      const known = new Set(state.order.map((id) => state.entries[id]?.input.path));
      const entries = { ...state.entries };
      const order = state.order.slice();
      let nextId = state.nextId;
      let added = 0;
      for (const file of result.files) {
        if (known.has(file.input.path)) continue;
        known.add(file.input.path);
        const entry = createEntry(nextId, file.input, file.info);
        if (file.info.error) {
          entry.status = "failed";
          entry.report = null;
        }
        entries[nextId] = entry;
        order.push(nextId);
        nextId += 1;
        added += 1;
      }
      set({ entries, order, nextId });
      if (added > 0) get().toast("success", "toast.added", { n: added });
      if (result.skipped > 0) get().toast("info", "toast.skippedScan", { n: result.skipped });
      if (added === 0 && result.skipped === 0) get().toast("info", "toast.noImages");
    } catch (error) {
      get().reportError(error);
    } finally {
      set({ scanning: false });
    }
  },

  async addFilesFromDialog(title) {
    try {
      await get().addPaths(await backend.pickFiles(title));
    } catch (error) {
      get().reportError(error);
    }
  },

  async addFolderFromDialog(title) {
    try {
      const folder = await backend.pickFolder(title);
      if (folder) await get().addPaths([folder]);
    } catch (error) {
      get().reportError(error);
    }
  },

  async chooseOutputFolder(title) {
    try {
      const folder = await backend.pickFolder(title);
      if (folder) {
        get().updateSettings((settings) => ({
          ...settings,
          output: { ...settings.output, location: { kind: "directory", path: folder } },
        }));
      }
    } catch (error) {
      get().reportError(error);
    }
  },

  removeEntry(id) {
    const { entries, order } = get();
    if (entries[id]?.status === "processing") return;
    const nextEntries = { ...entries };
    delete nextEntries[id];
    set({ entries: nextEntries, order: order.filter((other) => other !== id) });
  },

  clearAll() {
    if (get().job.running) return;
    set({ entries: {}, order: [], job: IDLE_JOB });
  },

  clearFinished() {
    const { entries, order } = get();
    const keep = order.filter((id) => entries[id]?.status !== "done");
    const nextEntries: Record<number, FileEntry> = {};
    for (const id of keep) {
      const entry = entries[id];
      if (entry) nextEntries[id] = entry;
    }
    set({ entries: nextEntries, order: keep });
  },

  setFilter(filter) {
    set({ filter });
  },

  setSort(sort) {
    set({ sort });
  },

  toggleDirection() {
    set({ direction: get().direction === "asc" ? "desc" : "asc" });
  },

  start(ids) {
    const state = get();
    if (state.job.running) {
      state.toast("info", "toast.alreadyRunning");
      return;
    }
    const all = state.order.map((id) => state.entries[id]).filter((entry): entry is FileEntry => Boolean(entry));
    const targets = ids
      ? ids.map((id) => state.entries[id]).filter((entry): entry is FileEntry => Boolean(entry))
      : filesToProcess(all);
    const runnable = targets.filter((entry) => !entry.info?.error);
    if (runnable.length === 0) return;
    const settings: OptimizeSettings = structuredClone(state.config.settings);
    const steps = preflightSteps(settings, runnable);
    const run: PendingRun = { ids: runnable.map((entry) => entry.id), settings, steps };
    if (steps.length > 0) {
      set({ dialog: { kind: "preflight", run } });
    } else {
      void launch(run);
    }
  },

  answerPreflight(choice) {
    const dialog = get().dialog;
    if (dialog?.kind !== "preflight") return;
    const [step, ...rest] = dialog.run.steps;
    if (!step || choice === "cancel") {
      set({ dialog: null });
      return;
    }
    const settings = dialog.run.settings;
    if (step.kind === "animation" && choice === "alternative") settings.animation = "firstFrame";
    if (step.kind === "transparency" && choice === "alternative") settings.transparency = "flattenWhite";
    const run: PendingRun = { ...dialog.run, settings, steps: rest };
    if (rest.length > 0) {
      set({ dialog: { kind: "preflight", run } });
    } else {
      set({ dialog: null });
      void launch(run);
    }
  },

  async cancelJob() {
    const { job } = get();
    if (!job.running || job.id === null) return;
    try {
      await backend.cancelJob(job.id);
    } catch (error) {
      get().reportError(error);
    }
  },

  async skipFile(id) {
    const { job } = get();
    if (!job.running || job.id === null) return;
    try {
      await backend.skipFile(job.id, id);
    } catch (error) {
      get().reportError(error);
    }
  },

  retryFailed() {
    const { entries, order } = get();
    const failed = order.filter((id) => {
      const entry = entries[id];
      return (entry?.status === "failed" || entry?.status === "cancelled") && !entry.info?.error;
    });
    if (failed.length > 0) get().start(failed);
  },

  applyEvents(events) {
    const { entries, job } = get();
    const next = reduceJobEvents(entries, job, events);
    set({ entries: next.entries, job: next.job });
  },

  openDialog(dialog) {
    set({ dialog });
  },

  closeDialog() {
    set({ dialog: null });
  },

  toast(kind, key, values, detail) {
    toastCounter += 1;
    const toast: Toast = { id: toastCounter, kind, key, values, detail };
    set({ toasts: [...get().toasts.slice(-3), toast] });
    const lifetime = kind === "error" ? 9000 : 4500;
    setTimeout(() => get().dismissToast(toast.id), lifetime);
  },

  dismissToast(id) {
    set({ toasts: get().toasts.filter((toast) => toast.id !== id) });
  },

  reportError(error) {
    const commandError: CommandError = backend.toCommandError(error);
    const key = `error.${commandError.code}` as MessageKey;
    get().toast("error", key, undefined, commandError.message);
  },

  async createZip(title) {
    const { entries, order, config } = get();
    const items = order
      .map((id) => entries[id])
      .filter((entry): entry is FileEntry => entry?.status === "done" && Boolean(entry.report?.outputPath))
      .map((entry) => ({ input: entry.input, output: entry.report?.outputPath ?? "" }));
    if (items.length === 0) {
      get().toast("info", "toast.nothingToZip");
      return;
    }
    try {
      const summary = await backend.createZipArchive(items, config.settings, title);
      if (summary) get().toast("success", "toast.zipCreated", { n: summary.files, size: formatBytes(summary.size) });
    } catch (error) {
      get().reportError(error);
    }
  },

  async exportEntry(id, title) {
    const output = get().entries[id]?.report?.outputPath;
    if (!output) return;
    try {
      const saved = await backend.exportFile(output, title);
      if (saved) get().toast("success", "toast.exported", { name: fileName(saved) });
    } catch (error) {
      get().reportError(error);
    }
  },

  async revealEntry(id) {
    const entry = get().entries[id];
    const path = entry?.report?.outputPath ?? entry?.input.path;
    if (!path) return;
    try {
      await backend.revealPath(path);
    } catch (error) {
      get().reportError(error);
    }
  },

  async openOutputFolder() {
    const { entries, order } = get();
    const done = order.map((id) => entries[id]).find((entry) => entry?.report?.outputPath);
    const output = done?.report?.outputPath;
    if (!output) return;
    try {
      await backend.openFolder(parentDir(output));
    } catch (error) {
      get().reportError(error);
    }
  },

  async openLogs() {
    const logDir = get().appInfo?.logDir;
    if (!logDir) return;
    try {
      await backend.openFolder(logDir);
    } catch (error) {
      get().reportError(error);
    }
  },
}));

/** Starts the backend job for a confirmed run. */
async function launch(run: PendingRun): Promise<void> {
  const store = useImagoStore;
  const state = store.getState();
  const tasks: FileTask[] = [];
  const entries = { ...state.entries };
  for (const id of run.ids) {
    const entry = entries[id];
    if (!entry) continue;
    tasks.push({ id, input: entry.input });
    entries[id] = { ...entry, status: "pending", stage: null, progress: 0, report: null };
  }
  if (tasks.length === 0) return;
  store.setState({
    entries,
    job: { ...IDLE_JOB, running: true, total: tasks.length, startedAt: Date.now() },
  });
  const buffer = createEventBuffer<JobEvent>((events) => store.getState().applyEvents(events), {
    isUrgent: (event) => event.type === "finished",
  });
  try {
    const jobId = await backend.startJob(tasks, run.settings, (event) => buffer.push(event));
    const job = store.getState().job;
    if (job.running) store.setState({ job: { ...job, id: jobId } });
  } catch (error) {
    buffer.flush();
    store.setState({ job: IDLE_JOB });
    store.getState().reportError(error);
  }
}
