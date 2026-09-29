import { useEffect, useState } from "react";

import { useT } from "../hooks/useT";
import { renderImage } from "../services/backend";
import { useImagoStore } from "../store/useImagoStore";
import { FORMAT_LABELS } from "../utils/defaults";
import { formatBytes, formatDimensions, formatReduction } from "../utils/format";
import { Modal } from "./Modal";

const PREVIEW_SIZE = 1600;

function usePreview(path: string | null | undefined): { url: string | null; failed: boolean } {
  const [state, setState] = useState<{ url: string | null; failed: boolean }>({ url: null, failed: false });
  useEffect(() => {
    let alive = true;
    let created: string | null = null;
    setState({ url: null, failed: false });
    if (!path) return undefined;
    renderImage(path, PREVIEW_SIZE)
      .then((url) => {
        created = url;
        if (alive) setState({ url, failed: false });
        else URL.revokeObjectURL(url);
      })
      .catch(() => alive && setState({ url: null, failed: true }));
    return () => {
      alive = false;
      if (created) URL.revokeObjectURL(created);
    };
  }, [path]);
  return state;
}

/** Before/after comparison with a draggable divider (keyboard accessible). */
export function PreviewDialog({ id }: { id: number }) {
  const t = useT();
  const entry = useImagoStore((state) => state.entries[id]);
  const closeDialog = useImagoStore((state) => state.closeDialog);
  const [split, setSplit] = useState(50);
  const before = usePreview(entry?.input.path);
  const after = usePreview(entry?.report?.outputPath);

  if (!entry) return null;
  const report = entry.report;

  return (
    <Modal title={`${t("preview.title")} — ${entry.name}`} onClose={closeDialog} wide>
      {!report?.outputPath ? (
        <p className="modal__text">{t("preview.notProcessed")}</p>
      ) : (
        <>
          <div className="compare">
            {before.url && after.url ? (
              <>
                <img className="compare__image" src={before.url} alt={t("preview.before")} />
                <img
                  className="compare__image compare__image--after"
                  src={after.url}
                  alt={t("preview.after")}
                  style={{ clipPath: `inset(0 0 0 ${split}%)` }}
                />
                <div className="compare__divider" style={{ left: `${split}%` }} aria-hidden="true" />
                <span className="compare__label compare__label--before">{t("preview.before")}</span>
                <span className="compare__label compare__label--after">{t("preview.after")}</span>
              </>
            ) : (
              <p className="compare__status">
                {before.failed || after.failed ? t("preview.unavailable") : t("preview.loading")}
              </p>
            )}
          </div>
          <label className="compare__slider">
            <span className="visually-hidden">{t("preview.compare")}</span>
            <input type="range" min={0} max={100} value={split} onChange={(event) => setSplit(Number(event.target.value))} />
          </label>
          <dl className="compare__facts">
            <div>
              <dt>{t("preview.before")}</dt>
              <dd>
                {report.inputFormat ? FORMAT_LABELS[report.inputFormat] : "—"} ·{" "}
                {formatDimensions(report.inputDimensions?.[0], report.inputDimensions?.[1])} · {formatBytes(report.inputSize)}
              </dd>
            </div>
            <div>
              <dt>{t("preview.after")}</dt>
              <dd>
                {report.outputFormat ? FORMAT_LABELS[report.outputFormat] : "—"} ·{" "}
                {formatDimensions(report.outputDimensions?.[0], report.outputDimensions?.[1])} · {formatBytes(report.outputSize)}{" "}
                <strong>{formatReduction(report.reductionPercent)}</strong>
              </dd>
            </div>
          </dl>
        </>
      )}
    </Modal>
  );
}
