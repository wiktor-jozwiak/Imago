import { useT } from "../hooks/useT";
import { type PendingRun, useImagoStore } from "../store/useImagoStore";
import { FORMAT_LABELS } from "../utils/defaults";
import { Modal } from "./Modal";

/**
 * Questions asked before a run: overwriting files, animations with a format
 * that cannot animate, transparency with JPEG. Nothing destructive happens
 * without an explicit answer.
 */
export function PreflightDialog({ run }: { run: PendingRun }) {
  const t = useT();
  const answer = useImagoStore((state) => state.answerPreflight);
  const step = run.steps[0];
  if (!step) return null;

  const cancel = () => answer("cancel");
  let title: string;
  let body: string;
  let primary: { label: string; choice: "continue" | "alternative"; danger?: boolean };
  let secondary: { label: string; choice: "continue" | "alternative" } | null = null;

  switch (step.kind) {
    case "overwrite":
      title = t("dialog.overwrite.title");
      body = t("dialog.overwrite.body");
      primary = { label: t("dialog.overwrite.confirm"), choice: "continue", danger: true };
      break;
    case "animation":
      title = t("dialog.animation.title");
      body = t("dialog.animation.body", { n: step.count, format: FORMAT_LABELS[step.format] });
      primary = { label: t("dialog.animation.keep"), choice: "continue" };
      secondary = { label: t("dialog.animation.firstFrame"), choice: "alternative" };
      break;
    case "transparency":
      title = t("dialog.transparency.title");
      body = t("dialog.transparency.body", { n: step.count });
      primary = { label: t("dialog.transparency.keep"), choice: "continue" };
      secondary = { label: t("dialog.transparency.flatten"), choice: "alternative" };
      break;
  }

  return (
    <Modal title={title} onClose={cancel}>
      <p className="modal__text">{body}</p>
      <div className="modal__actions">
        <button type="button" className="button button--ghost" onClick={cancel}>
          {t("dialog.cancel")}
        </button>
        {secondary && (
          <button type="button" className="button" onClick={() => answer(secondary.choice)}>
            {secondary.label}
          </button>
        )}
        <button
          type="button"
          className={`button ${primary.danger ? "button--danger" : "button--primary"}`}
          onClick={() => answer(primary.choice)}
        >
          {primary.label}
        </button>
      </div>
    </Modal>
  );
}
