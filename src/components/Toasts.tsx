import { useT } from "../hooks/useT";
import { useImagoStore } from "../store/useImagoStore";
import { IconAlert, IconCheck, IconClose } from "./icons";

/** Short, dismissible notifications (errors stay longer). */
export function Toasts() {
  const t = useT();
  const toasts = useImagoStore((state) => state.toasts);
  const dismiss = useImagoStore((state) => state.dismissToast);
  return (
    <div className="toasts" aria-live="polite" aria-atomic="false">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast toast--${toast.kind}`} role={toast.kind === "error" ? "alert" : "status"} title={toast.detail}>
          {toast.kind === "error" ? <IconAlert size={16} /> : <IconCheck size={16} />}
          <span className="toast__text">{t(toast.key, toast.values)}</span>
          <button type="button" className="icon-button icon-button--small" onClick={() => dismiss(toast.id)} aria-label={t("actions.close")}>
            <IconClose size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
