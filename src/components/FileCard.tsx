import { memo } from "react";

import { useT } from "../hooks/useT";
import { useThumbnail } from "../hooks/useThumbnail";
import { type MessageKey, isMessageKey } from "../i18n";
import { useImagoStore } from "../store/useImagoStore";
import type { FileReport } from "../types/imago";
import { FORMAT_LABELS } from "../utils/defaults";
import type { FileEntry } from "../utils/files";
import { formatBytes, formatDimensions, formatDuration, formatReduction, parentDir } from "../utils/format";
import { IconAlert, IconDownload, IconEye, IconImage, IconReveal, IconRetry, IconSkip, IconTrash } from "./icons";
import { ProgressBar } from "./ProgressBar";

/** Human-readable explanation of the output format decision. */
export function useDecisionText(): (report: FileReport) => string | null {
  const t = useT();
  return (report) => {
    const decision = report.decision;
    if (!decision) return null;
    const format = FORMAT_LABELS[decision.format];
    switch (decision.reason) {
      case "smallest": {
        let text = t("decision.smallest", { format, percent: Math.max(0, Math.round(decision.savedPercent)) });
        if (decision.keptTransparency) text += t("decision.keptTransparency");
        if (decision.keptAnimation) text += t("decision.keptAnimation");
        return `${text}.`;
      }
      case "keptOriginal":
        return t("decision.keptOriginal");
      case "requested":
        return t("decision.requested", { format });
    }
  };
}

const NEWLINE = "\n";

function messageFor(code: string, prefix: "error" | "warning", fallback: string, t: ReturnType<typeof useT>): string {
  const key = `${prefix}.${code}`;
  return isMessageKey(key) ? t(key as MessageKey) : fallback;
}

/** One file row: thumbnail, name, formats, dimensions, sizes with the reduction, status and actions. */
export const FileCard = memo(function FileCard({ entry, jobRunning }: { entry: FileEntry; jobRunning: boolean }) {
  const t = useT();
  const decisionText = useDecisionText();
  const thumbnail = useThumbnail(entry.input.path);
  const start = useImagoStore((state) => state.start);
  const skipFile = useImagoStore((state) => state.skipFile);
  const removeEntry = useImagoStore((state) => state.removeEntry);
  const revealEntry = useImagoStore((state) => state.revealEntry);
  const exportEntry = useImagoStore((state) => state.exportEntry);
  const openDialog = useImagoStore((state) => state.openDialog);

  const { report, info, status } = entry;
  const inputFormat = report?.inputFormat ?? info?.format ?? null;
  const outputFormat = report?.outputFormat ?? null;
  const dimensions = report?.inputDimensions ?? (info?.width && info.height ? [info.width, info.height] : null);
  const outputDimensions = report?.outputDimensions;
  const resized =
    outputDimensions && dimensions && (outputDimensions[0] !== dimensions[0] || outputDimensions[1] !== dimensions[1]);
  const error = report?.error ?? info?.error ?? null;
  const reduction = report?.reductionPercent ?? null;
  const decision = report && status === "done" ? decisionText(report) : null;
  const warnings = status === "done" ? (report?.warnings ?? []) : [];

  const errorText =
    error && (status === "failed" || status === "skipped")
      ? messageFor(error.code, "error", error.message, t) +
        (error.alternative ? ` ${t("error.tryAlternative", { format: error.alternative })}` : "")
      : null;
  const warningTexts = warnings.map((warning) => messageFor(warning.code, "warning", warning.message, t));

  // One line keeps rows the same height (the list is virtualized); the full text is in the tooltip.
  const note: { tone: "error" | "warning" | "info"; text: string; full: string } | null = errorText
    ? { tone: "error", text: errorText, full: [errorText, error?.message].filter(Boolean).join(NEWLINE) }
    : warningTexts.length > 0
      ? { tone: "warning", text: warningTexts[0] ?? "", full: [...warningTexts, decision].filter(Boolean).join(NEWLINE) }
      : decision
        ? { tone: "info", text: decision, full: decision }
        : null;

  const inputSize = report?.inputSize ?? entry.input.size;
  const outputSize = status === "done" ? (report?.outputSize ?? null) : null;

  return (
    <div className={`file-card file-card--${status}`} role="listitem" aria-label={entry.name}>
      <div className="file-card__thumb">
        {thumbnail ? <img src={thumbnail} alt="" /> : <IconImage size={22} />}
      </div>

      <div className="file-card__main">
        <div className="file-card__title">
          <span className="file-card__name" title={entry.input.path}>
            {entry.name}
          </span>
          {info?.animated && <span className="tag">{t("file.animated")}</span>}
          {info?.hasAlpha && !info.animated && <span className="tag">{t("file.alpha")}</span>}
        </div>
        <div className="file-card__meta">
          <span className="file-card__formats">
            {inputFormat ? FORMAT_LABELS[inputFormat] : t("file.formatUnknown")}
            {outputFormat && <> → {FORMAT_LABELS[outputFormat]}</>}
          </span>
          <span title={t("file.dimensions")}>
            {formatDimensions(dimensions?.[0], dimensions?.[1])}
            {resized && outputDimensions && <> → {formatDimensions(outputDimensions[0], outputDimensions[1])}</>}
          </span>
          {report && status !== "pending" && report.timings.totalMs > 0 && (
            <span title={t("file.time")}>{formatDuration(report.timings.totalMs)}</span>
          )}
          <span className="file-card__path" title={entry.input.path}>
            {parentDir(entry.input.path)}
          </span>
        </div>
        {status === "processing" ? (
          <div className="file-card__progress">
            <ProgressBar value={entry.progress} label={t("progress.file")} size="thin" />
            {entry.stage && <span className="file-card__stage">{t(`stage.${entry.stage}`)}</span>}
          </div>
        ) : note ? (
          <p className={`file-card__note file-card__note--${note.tone}`} title={note.full}>
            {note.tone !== "info" && <IconAlert size={14} />}
            <span className="file-card__note-text">{note.text}</span>
          </p>
        ) : null}
      </div>

      <div className="file-card__sizes" title={t("file.size")}>
        {reduction !== null && status === "done" && (
          <span className={`badge ${reduction >= 0 ? "badge--success" : "badge--warning"}`}>{formatReduction(reduction)}</span>
        )}
        <span className="file-card__size">
          {outputSize !== null ? (
            <>
              <span className="file-card__size-old">{formatBytes(inputSize)}</span> → <strong>{formatBytes(outputSize)}</strong>
            </>
          ) : (
            formatBytes(inputSize)
          )}
        </span>
      </div>

      <span className={`pill pill--${status}`}>
        <span className="pill__dot" aria-hidden="true" />
        {t(`status.${status}`)}
      </span>

      <div className="file-card__actions">
        {status === "done" && (
          <>
            <button type="button" className="icon-button" onClick={() => openDialog({ kind: "preview", id: entry.id })} aria-label={t("actions.preview")} title={t("actions.preview")}>
              <IconEye />
            </button>
            <button type="button" className="icon-button" onClick={() => void revealEntry(entry.id)} aria-label={t("actions.reveal")} title={t("actions.reveal")}>
              <IconReveal />
            </button>
            <button type="button" className="icon-button" onClick={() => void exportEntry(entry.id, t("actions.saveAs"))} aria-label={t("actions.saveAs")} title={t("actions.saveAs")}>
              <IconDownload />
            </button>
          </>
        )}
        {(status === "failed" || status === "cancelled" || status === "skipped") && !jobRunning && !info?.error && (
          <button type="button" className="icon-button" onClick={() => start([entry.id])} aria-label={t("actions.retry")} title={t("actions.retry")}>
            <IconRetry />
          </button>
        )}
        {jobRunning && (status === "processing" || status === "pending") && (
          <button type="button" className="icon-button" onClick={() => void skipFile(entry.id)} aria-label={t("actions.skip")} title={t("actions.skip")}>
            <IconSkip />
          </button>
        )}
        {status !== "processing" && !(jobRunning && status === "pending") && (
          <button type="button" className="icon-button" onClick={() => removeEntry(entry.id)} aria-label={t("actions.remove")} title={t("actions.remove")}>
            <IconTrash />
          </button>
        )}
      </div>
    </div>
  );
});
