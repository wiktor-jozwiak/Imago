import { useEffect } from "react";

import { useImagoStore } from "../store/useImagoStore";

/** Applies the chosen theme (`system` follows the OS through CSS media queries). */
export function useThemeEffect(): void {
  const theme = useImagoStore((state) => state.config.ui.theme);
  const language = useImagoStore((state) => state.language);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "system") {
      delete root.dataset.theme;
    } else {
      root.dataset.theme = theme;
    }
  }, [theme]);

  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
}
