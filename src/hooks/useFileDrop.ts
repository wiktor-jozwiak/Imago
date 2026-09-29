import { useEffect } from "react";

import { onFileDrop } from "../services/backend";
import { useImagoStore } from "../store/useImagoStore";

/** Connects native window drag & drop (files and folders) to the store. */
export function useFileDrop(): void {
  useEffect(() => {
    let unsubscribe: (() => void) | null = null;
    let disposed = false;
    onFileDrop({
      over: (active) => useImagoStore.getState().setDragActive(active),
      drop: (paths) => void useImagoStore.getState().addPaths(paths),
    })
      .then((stop) => {
        if (disposed) stop();
        else unsubscribe = stop;
      })
      .catch((error: unknown) => useImagoStore.getState().reportError(error));
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);
}
