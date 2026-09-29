import { useT } from "../hooks/useT";
import { useImagoStore } from "../store/useImagoStore";
import type { Theme } from "../types/imago";
import { IconMonitor, IconMoon, IconSun } from "./icons";

const NEXT: Record<Theme, Theme> = { system: "light", light: "dark", dark: "system" };

/** Cycles system → light → dark. */
export function ThemeToggle() {
  const t = useT();
  const theme = useImagoStore((state) => state.config.ui.theme);
  const setTheme = useImagoStore((state) => state.setTheme);
  const name = t(`theme.${theme}`);
  const Icon = theme === "light" ? IconSun : theme === "dark" ? IconMoon : IconMonitor;
  return (
    <button
      type="button"
      className="icon-button"
      onClick={() => setTheme(NEXT[theme])}
      aria-label={t("theme.toggle", { name })}
      title={t("theme.toggle", { name })}
    >
      <Icon />
    </button>
  );
}
