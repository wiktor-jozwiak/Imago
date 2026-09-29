import { en } from "./en";
import { isMessageKey, resolveLanguage, translator } from "./index";
import { pl } from "./pl";

describe("i18n", () => {
  it("has the same keys and no empty texts in every language", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(pl).sort());
    for (const [key, text] of [...Object.entries(pl), ...Object.entries(en)]) {
      expect(text.trim(), key).not.toBe("");
    }
  });

  it("uses the same placeholders in both languages", () => {
    const placeholders = (text: string) => (text.match(/\{\w+\}/g) ?? []).sort().join(",");
    for (const key of Object.keys(pl) as (keyof typeof pl)[]) {
      expect(placeholders(en[key]), key).toBe(placeholders(pl[key]));
    }
  });

  it("interpolates values", () => {
    const t = translator("pl");
    expect(t("summary.running", { done: 3, total: 10 })).toBe("Optymalizacja: 3 z 10");
    expect(translator("en")("toast.added", { n: 5 })).toBe("Files added: 5");
  });

  it("covers every engine error and warning code", () => {
    const errors = [
      "not_found", "permission_denied", "disk_full", "io_error", "not_a_file", "file_too_large",
      "dimensions_too_large", "unsupported_format", "format_unavailable", "decode_failed", "encode_failed",
      "output_exists", "would_overwrite_original", "path_traversal", "verification_failed", "invalid_settings",
      "cancelled", "archive_error", "config_error", "internal_error", "skipped_by_user", "path_not_allowed",
    ];
    for (const code of errors) expect(isMessageKey(`error.${code}`), code).toBe(true);
    const warnings = [
      "extension_mismatch", "animation_kept_original", "animation_kept_in_input_format", "resize_not_applied",
      "animation_first_frame", "quality_not_used", "transparency_flattened", "transparency_kept_in_other_format",
      "icc_ignored", "icc_converted_to_srgb", "larger_than_original", "permissions_not_copied",
      "timestamps_not_copied", "output_extension_changed",
    ];
    for (const code of warnings) expect(isMessageKey(`warning.${code}`), code).toBe(true);
  });

  it("resolves the language from preference or system", () => {
    expect(resolveLanguage("en", ["pl-PL"])).toBe("en");
    expect(resolveLanguage(null, ["pl-PL", "en-US"])).toBe("pl");
    expect(resolveLanguage(null, ["de-DE"])).toBe("en");
  });
});
