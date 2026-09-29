import { useMemo } from "react";

import { useT } from "../hooks/useT";
import { overallProgress } from "../store/jobReducer";
import { useImagoStore } from "../store/useImagoStore";
import type { FileEntry } from "../utils/files";
import { filesToProcess } from "../utils/files";
import { formatBytes, formatDuration } from "../utils/format";
import { IconArchive, IconFolder, IconPlay, IconRetry, IconStop } from "./icons";
import { ProgressBar } from "./ProgressBar";

interface Totals {
  done: number;
  skipped: number;
  failed: number;
  inputBytes: number;
  outputBytes: number;
}

function totalsOf(entries: FileEntry[]): Totals {
  const totals: Totals = { done: 0, skipped: 0, failed: 0, inputBytes: 0, outputBytes: 0 };
  for (const entry of entries) {
    if (entry.status === "done" && entry.report) {
      totals.done += 1;
      totals.inputBytes += entry.report.inputSize;
      totals.outputBytes += entry.report.outputSize ?? entry.report.inputSize;
    } else if (entry.status === "skipped") {
      totals.skipped += 1;
    } else if (entry.status === "failed") {
      totals.failed += 1;
    }
  }
  return totals;
}

/** Totals, global progress and the main actions. */
export function SummaryPanel() {
  const t = useT();
  const entriesById = useImagoStore((state) => state.entries);
  const order = useImagoStore((state) => state.order);
  const job = useImagoStore((state) => state.job);
  const start = useImagoStore((state) => state.start);
  const cancelJob = useImagoStore((state) => state.cancelJob);
  const retryFailed = useImagoStore((state) => state.retryFailed);
  const createZip = useImagoStore((state) => state.createZip);
  const openOutputFolder = useImagoStore((state) => state.openOutputFolder);

  const entries = useMemo(
    () => order.map((id) => entriesById[id]).filter((entry): entry is FileEntry => Boolean(entry)),
    [entriesById, order],
  );
  const totals = useMemo(() => totalsOf(entries), [entries]);
  const progress = overallProgress(entries, job);
  const saved = totals.inputBytes - totals.outputBytes;
  const savedPercent = totals.inputBytes > 0 ? Math.round((saved / totals.inputBytes) * 100) : 0;
  const pending = filesToProcess(entries).filter((entry) => !entry.info?.error).length;
  // Files rejected while scanning (not an image, too large, ...) cannot be retried.
  const retryable = entries.filter(
    (entry) => (entry.status === "failed" || entry.status === "cancelled") && !entry.info?.error,
  ).length;
  const jobFailed = (job.summary?.failed ?? 0) > 0;

  let headline: string;
  if (job.running) headline = t("summary.running", { done: job.finished, total: job.total });
  else if (job.summary?.wasCancelled) headline = t("summary.cancelled");
  else if (job.summary) headline = t("summary.finished");
  else if (entries.length > 0) headline = t("summary.ready", { n: pending });
  else headline = t("summary.empty");

  const showProgress = job.running || job.summary !== null;

  return (
    <footer className="summary-panel">
      {showProgress && (
        <div className="summary-panel__progress">
          <ProgressBar
            value={job.running ? progress : 1}
            label={t("progress.global")}
            size="thin"
            tone={job.running ? "accent" : jobFailed ? "danger" : "success"}
          />
        </div>
      )}

      {totals.done > 0 && (
        <div className="saved-hero">
          <span className="saved-hero__label">{t("summary.savedHero")}</span>
          <span className="saved-hero__value">
            {formatBytes(Math.max(0, saved))}
            {savedPercent > 0 && <span className="saved-hero__percent">−{savedPercent}%</span>}
          </span>
          <span className="saved-hero__of">{t("summary.of", { size: formatBytes(totals.inputBytes) })}</span>
        </div>
      )}

      <div className="summary-panel__status">
        <div className="summary-panel__headline" aria-live="polite">
          {headline}
        </div>
        {(entries.length > 0 || job.summary) && (
          <dl className="stats">
            <div>
              <dt>{t("summary.processed")}</dt>
              <dd>{totals.done}</dd>
            </div>
            <div>
              <dt>{t("summary.skipped")}</dt>
              <dd>{totals.skipped}</dd>
            </div>
            <div>
              <dt>{t("summary.errors")}</dt>
              <dd className={totals.failed > 0 ? "is-danger" : undefined}>{totals.failed}</dd>
            </div>
            <div>
              <dt>{t("summary.time")}</dt>
              <dd>{job.summary ? formatDuration(job.summary.elapsedMs) : job.startedAt ? "…" : "—"}</dd>
            </div>
          </dl>
        )}
      </div>

      <div className="summary-panel__actions">
        {totals.done > 0 && !job.running && (
          <>
            <button type="button" className="button button--ghost" onClick={() => void openOutputFolder()}>
              <IconFolder /> {t("actions.openOutput")}
            </button>
            <button type="button" className="button button--ghost" onClick={() => void createZip(t("actions.zip"))}>
              <IconArchive /> {t("actions.zip")}
            </button>
          </>
        )}
        {retryable > 0 && !job.running && (
          <button type="button" className="button button--ghost" onClick={retryFailed}>
            <IconRetry /> {t("actions.retryFailed")}
          </button>
        )}
        {job.running ? (
          <button type="button" className="button button--danger" onClick={() => void cancelJob()} title="Esc">
            <IconStop /> {t("actions.cancel")}
          </button>
        ) : (
          <button
            type="button"
            className="button button--large button--cta"
            onClick={() => start()}
            disabled={pending === 0}
            title="Ctrl+Enter"
          >
            <IconPlay /> {t("actions.start")}
          </button>
        )}
      </div>
    </footer>
  );
}
