import { en } from "./en";
import { pl, type MessageKey, type Messages } from "./pl";

export type Language = "pl" | "en";
export type { MessageKey };

const DICTIONARIES: Record<Language, Messages> = { pl, en };

/** Picks the interface language: saved preference, then the system language. */
export function resolveLanguage(saved: string | null | undefined, systemLanguages: readonly string[]): Language {
  if (saved === "pl" || saved === "en") return saved;
  const polish = systemLanguages.some((language) => language.toLowerCase().startsWith("pl"));
  return polish ? "pl" : "en";
}

export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

/** Returns a translator for `language`. Unknown placeholders are left visible. */
export function translator(language: Language): Translate {
  const messages = DICTIONARIES[language];
  return (key, values) => {
    const template = messages[key] ?? key;
    if (!values) return template;
    return template.replace(/\{(\w+)\}/g, (match, name: string) =>
      Object.prototype.hasOwnProperty.call(values, name) ? String(values[name]) : match,
    );
  };
}

/** Whether a string is a known message key (for error and warning codes). */
export function isMessageKey(key: string): key is MessageKey {
  return Object.prototype.hasOwnProperty.call(pl, key);
}
