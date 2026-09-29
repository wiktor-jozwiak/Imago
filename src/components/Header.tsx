import { useT } from "../hooks/useT";
import type { Language } from "../i18n";
import { useImagoStore } from "../store/useImagoStore";
import { IconKeyboard } from "./icons";
import { ThemeToggle } from "./ThemeToggle";

/** Product name, tagline, language and theme controls. */
export function Header() {
  const t = useT();
  const language = useImagoStore((state) => state.language);
  const setLanguage = useImagoStore((state) => state.setLanguage);
  const openDialog = useImagoStore((state) => state.openDialog);
  const version = useImagoStore((state) => state.appInfo?.info.version);

  return (
    <header className="app-header">
      <div className="brand">
        <img className="brand__logo" src="/imago.svg" alt="" width={36} height={36} />
        <div className="brand__text">
          <h1 className="brand__name">
            Imago{version && <span className="brand__version">{version}</span>}
          </h1>
          <p className="brand__tagline">
            <span lang="en">Fast. Local. Smaller images.</span>
            {language === "pl" && <span className="brand__tagline-alt"> · {t("app.tagline")}</span>}
          </p>
        </div>
      </div>
      <div className="app-header__actions">
        <div className="language-switch" role="group" aria-label={t("language.label")}>
          {(["pl", "en"] as Language[]).map((code) => (
            <button
              key={code}
              type="button"
              className={`language-switch__option${language === code ? " is-selected" : ""}`}
              aria-pressed={language === code}
              onClick={() => setLanguage(code)}
            >
              {code.toUpperCase()}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="icon-button"
          onClick={() => openDialog({ kind: "shortcuts" })}
          aria-label={t("shortcuts.open")}
          title={t("shortcuts.open")}
        >
          <IconKeyboard />
        </button>
        <ThemeToggle />
      </div>
    </header>
  );
}
