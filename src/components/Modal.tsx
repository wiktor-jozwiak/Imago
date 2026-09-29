import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef } from "react";

import { useT } from "../hooks/useT";
import { IconClose } from "./icons";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Accessible modal dialog: labelled, focus kept inside, focus restored on close. */
export function Modal(props: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  const t = useT();
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = dialogRef.current;
    const first = dialog?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    return () => previous?.focus?.();
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab" || !dialogRef.current) return;
    const focusable = Array.from(dialogRef.current.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && props.onClose()}>
      <div
        ref={dialogRef}
        className={`modal${props.wide ? " modal--wide" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={onKeyDown}
      >
        <div className="modal__header">
          <h2 id={titleId} className="modal__title">
            {props.title}
          </h2>
          <button type="button" className="icon-button" onClick={props.onClose} aria-label={t("actions.close")}>
            <IconClose />
          </button>
        </div>
        <div className="modal__body">{props.children}</div>
      </div>
    </div>
  );
}
