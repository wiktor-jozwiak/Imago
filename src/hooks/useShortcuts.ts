import { useEffect } from "react";

import type { Theme } from "../types/imago";
import { useImagoStore } from "../store/useImagoStore";
import { useT } from "./useT";

const THEME_CYCLE: Theme[] = ["system", "light", "dark"];

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

/**
 * Global keyboard shortcuts:
 * Ctrl/⌘+O add files · Ctrl/⌘+Shift+O add folder · Ctrl/⌘+Enter start ·
 * Esc cancel job / close dialog · Ctrl/⌘+Shift+L theme · ? shortcuts help.
 */
export function useShortcuts(): void {
  const t = useT();

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const store = useImagoStore.getState();
      const modifier = event.ctrlKey || event.metaKey;
      const key = event.key.toLowerCase();

      if (event.key === "Escape") {
        if (store.dialog) {
          store.closeDialog();
          event.preventDefault();
        } else if (store.job.running) {
          void store.cancelJob();
          event.preventDefault();
        }
        return;
      }
      if (store.dialog) return;

      if (modifier && key === "o") {
        event.preventDefault();
        if (event.shiftKey) {
          void store.addFolderFromDialog(t("actions.addFolder"));
        } else {
          void store.addFilesFromDialog(t("actions.addFiles"));
        }
      } else if (modifier && event.key === "Enter") {
        event.preventDefault();
        store.start();
      } else if (modifier && event.shiftKey && key === "l") {
        event.preventDefault();
        const current = THEME_CYCLE.indexOf(store.config.ui.theme);
        store.setTheme(THEME_CYCLE[(current + 1) % THEME_CYCLE.length] ?? "system");
      } else if (event.key === "?" && !isTyping(event.target)) {
        event.preventDefault();
        store.openDialog({ kind: "shortcuts" });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [t]);
}
