import { useMemo } from "react";

import { type Translate, translator } from "../i18n";
import { useImagoStore } from "../store/useImagoStore";

/** Translator for the current interface language. */
export function useT(): Translate {
  const language = useImagoStore((state) => state.language);
  return useMemo(() => translator(language), [language]);
}
