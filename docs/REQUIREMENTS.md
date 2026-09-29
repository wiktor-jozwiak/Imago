# Imago — macierz wymagań

> Mapowanie każdego wymagania z [SPEC.md](SPEC.md) na implementację i testy.
> Legenda: ✅ zrobione i przetestowane · 🟡 częściowo (opis w kolumnie „Uwagi”) · ➖ świadomie inaczej (ADR)
>
> Skróty ścieżek: `core/` = `crates/imago-core/src/`, `ui/` = `src/`, `tauri/` = `src-tauri/src/`.

## Funkcje aplikacji (1–55)

| # | Funkcja | Implementacja | Test / weryfikacja | Status |
|---|---|---|---|---|
| 1 | Przeciąganie plików | `ui/hooks/useFileDrop.ts` (natywny drag&drop Tauri) → `scan_paths` | E2E (ręcznie) | ✅ |
| 2 | Przeciąganie folderów | j.w., `core/filesystem/scan.rs` | `tests/scan.rs` | ✅ |
| 3 | Wybór plików (natywne okno) | `tauri/commands.rs::pick_files` | E2E UI Automation | ✅ |
| 4 | Wybór folderu (natywne okno) | `pick_folder` | E2E UI Automation (folder dodany, 5 plików) | ✅ |
| 5 | Pojedynczy plik | `jobs`, CLI `-o plik` | `tests/cli.rs` | ✅ |
| 6 | Wiele plików | `workers.rs::run_job` | `tests/pipeline.rs` (24 pliki / 4 wątki) | ✅ |
| 7 | Całe foldery | `scan_inputs` | `tests/scan.rs` | ✅ |
| 8 | Podfoldery | `scan_inputs` + zachowanie struktury | `unicode_names_and_structure_are_preserved` | ✅ |
| 9 | Rekurencyjne skanowanie (opcja) | `ScanOptions::recursive`, UI zaawansowane, CLI `-r` | `non_recursive_scan_stays_in_the_folder` | ✅ |
| 10 | Filtrowanie nieobsługiwanych | rozszerzenie + sygnatura | `recursive_scan_finds_nested_images` | ✅ |
| 11 | Pomijanie ukrytych (opcja) | kropka + atrybut Windows | `hidden_files_are_optional`, `windows_hidden_attribute_is_respected` | ✅ |
| 12 | Globalny pasek postępu | `ui/components/SummaryPanel.tsx`, `overallProgress` | `jobReducer.test.ts` | ✅ |
| 13 | Pasek postępu per plik | `FileCard.tsx`, zdarzenia `fileProgress` z etapem | `jobReducer.test.ts` | ✅ |
| 14 | Anulowanie zadania | `JobControl::cancel`, `cancel_job`, Esc, Ctrl+C w CLI | `cancelling_the_job_stops_every_file` | ✅ |
| 15 | Pominięcie pliku | `JobControl::skip`, przycisk w wierszu | `skipping_one_file_keeps_the_others` | ✅ |
| 16 | Ponowienie | przycisk „Ponów” / „Ponów nieudane” (nowe zadanie) | E2E, `preflight.test.ts` | ✅ |
| 17 | Wybór formatu | `FormatChoice::Fixed` | `fixed_format_conversion_keeps_dimensions` | ✅ |
| 18 | Oryginalny format | `FormatChoice::Original` (BMP → PNG) | `original_format_mode_turns_bmp_into_png` | ✅ |
| 19 | Automatyczny wybór | `core/image_engine/auto.rs` | `auto_mode_processes_a_mixed_batch_without_growing_files` | ✅ |
| 20 | Jakość 1–100 | `Quality` (walidowany newtype), suwak + pole | `quality_rejects_out_of_range_values`, `lower_quality_gives_smaller_files` | ✅ |
| 21 | Tryb bezstratny | `CompressionSettings::lossless` | `lossless_formats_preserve_pixels_exactly` | ✅ |
| 22 | Maksymalna kompresja | `Effort::Max` | `every_effort_level_produces_valid_output` | ✅ |
| 23 | Szerokość | `ResizeMode::Dimensions` | `width_only_keeps_aspect_ratio` | ✅ |
| 24 | Wysokość | j.w. | `height_only_keeps_aspect_ratio` | ✅ |
| 25 | Proporcje | `keep_aspect` | `both_dimensions_with/without_aspect…` | ✅ |
| 26 | Maks. szerokość | `ResizeMode::Fit` | `fit_only_shrinks_images_over_the_limit` | ✅ |
| 27 | Maks. wysokość | j.w. | j.w. | ✅ |
| 28 | Skalowanie procentowe | `ResizeMode::Percent` | `percent_scales_both_dimensions` | ✅ |
| 29 | Obrót wg EXIF | `apply_orientation` + reset tagu | `exif_orientation_is_applied_and_metadata_stripped` | ✅ |
| 30 | Usuwanie EXIF | `strip_metadata`, bezstratne czyszczenie kontenera | `metadata_is_removed_without_touching_pixels` | ✅ |
| 31 | Opcjonalne ICC | `keep_icc`, konwersja do sRGB (moxcms) | `icc_profiles_are_embedded`, `p3_red_becomes_a_different_srgb_value` | ✅ |
| 32 | Data modyfikacji | `preserve_timestamps` | `timestamps_can_be_preserved` | ✅ |
| 33 | Folder wyjściowy | `OutputLocation::Directory`, dialog | `folder_structure_is_preserved_below_the_root` | ✅ |
| 34 | Folder `optimized` | `OutputLocation::Subfolder` (domyślnie) | `default_output_goes_to_optimized_subfolder`, E2E | ✅ |
| 35 | Pomijanie istniejących | `ConflictPolicy::Skip` | `name_conflicts_follow_the_policy` | ✅ |
| 36 | Nadpisywanie po potwierdzeniu | `ConflictPolicy::Overwrite` + dialog | j.w., `preflight.test.ts` | ✅ |
| 37 | Przyrostek nazw | `OutputSettings::suffix` | `suffix_and_extension_spelling` | ✅ |
| 38 | Struktura folderów | `preserve_structure` | `unicode_names_and_structure_are_preserved` | ✅ |
| 39 | Pobieranie pojedynczych wyników | „Zapisz jako…” → `export_file` | `export_copies_and_protects_existing_files` | ✅ |
| 40 | Paczka ZIP | `core/archive.rs`, `create_zip_archive`, CLI `--zip` | `creates_a_readable_archive_with_unique_names`, `zip_and_json_report` | ✅ |
| 41 | Podgląd przed/po | `PreviewDialog.tsx`, `render_image` | E2E (zrzut) | ✅ |
| 42 | Wymiary | `FileInfo`, `FileReport` | E2E | ✅ |
| 43 | Rozmiar przed/po | `FileReport` | E2E | ✅ |
| 44 | Procent redukcji | `reduction_percent` | `summary_counts_statuses_and_bytes` | ✅ |
| 45 | Czas przetwarzania | `StageTimings` | E2E, CLI `-v` | ✅ |
| 46 | Liczba przetworzonych | `SummaryPanel`, `JobSummary` | E2E | ✅ |
| 47 | Liczba pominiętych | j.w. | j.w. | ✅ |
| 48 | Liczba błędów | j.w. | j.w. | ✅ |
| 49 | Tryb jasny | `tokens.css`, `ThemeToggle` | E2E (zrzut) | ✅ |
| 50 | Tryb ciemny | j.w. | E2E (zrzut) | ✅ |
| 51 | Zapisywanie ustawień | `core/config.rs`, `save_config` | `saved_configuration_round_trips`, plik `config.json` | ✅ |
| 52 | Skróty klawiszowe | `useShortcuts.ts` (Ctrl+O, Ctrl+Shift+O, Ctrl+Enter, Esc, Ctrl+Shift+L, ?) | — (ręcznie) | ✅ |
| 53 | Sortowanie | `visibleEntries` | `files.test.ts` | ✅ |
| 54 | Filtrowanie po statusie | chipy statusów | `App.test.tsx` | ✅ |
| 55 | Otwieranie folderu wynikowego | `open_folder` (plugin opener) | E2E (przycisk) | ✅ |

## Formaty

| Wymaganie | Status | Uwagi |
|---|---|---|
| Wejście: JPG/JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, JPEG XL | ✅ | `every_input_format_is_detected_probed_and_decoded` |
| Wyjście: JPEG, PNG, WebP, AVIF, TIFF | ✅ | `tests/codecs.rs` |
| Wyjście: JPEG XL | 🟡 | tylko bezstratnie (brak stabilnego stratnego kodera bez CMake), weryfikacja round-trip — [LIMITATIONS](LIMITATIONS.md) |
| Animacje GIF/WebP: wykrywanie, zachowanie domyślne, pierwsza klatka tylko za zgodą, ostrzeżenie | ✅ | `animations_are_preserved_unless_first_frame_is_allowed`, dialog przed startem |
| Format niedostępny: brak awarii, komunikat, alternatywa, log, instrukcja | ✅ | `capabilities.rs`, `unsupported_modes_are_reported_not_faked`, build `--no-default-features` |

## Tryb automatyczny

| Wymaganie | Status |
|---|---|
| Analiza typu, alfy, animacji, wymiarów, rozmiaru | ✅ `auto::analyze`, `probe` |
| Zasady wyboru (zdjęcia / alfa / grafiki / ikony / animacje / duże zdjęcia) | ✅ `auto::candidates` + testy |
| Nie zapisuje większego wyniku (chyba że wymuszono) | ✅ `WhenLarger`, test auto |
| Zachowuje przezroczystość i proporcje, nie usuwa alfy | ✅ |
| Nie skaluje poniżej limitów, unika zbędnego rekodowania | ✅ `Fit`, kandydat „oryginał” |
| Komunikat z uzasadnieniem | ✅ „Wybrano WebP, ponieważ zmniejszył rozmiar pliku o 98% i zachował przezroczystość.” (E2E) |
| Domyślne ustawienia (q=82, rdzenie−1, proporcje, bez metadanych, auto, `optimized`, oryginały chronione) | ✅ `defaults_match_the_specification` |

## Pipeline, wydajność, bezpieczeństwo

| Wymaganie | Status |
|---|---|
| 23 kroki z błędami, anulowaniem, pomiarem czasu, logowaniem | ✅ [PIPELINE.md](PIPELINE.md) |
| Kolejka, workerzy (konfigurowalni), limit jednoczesnych dekodowań | ✅ `workers.rs`, budżet pamięci |
| Pomiar czasów dekodowania/skalowania/kodowania/całości | ✅ `StageTimings` |
| Przetwarzanie strumieniowe | ✅ animacje klatka po klatce, ZIP strumieniowo |
| Limity dużych obrazów, ochrona przed bombami | ✅ `decompression_bomb_is_rejected_from_the_header` |
| Unicode, path traversal, format po treści | ✅ `tests/security.rs`, `unicode_*` |
| Bezpieczny zapis (tmp → fsync → weryfikacja → atomowy rename) | ✅ `safe_write.rs` + testy |
| Oryginały bez zgody nietknięte | ✅ `originals_are_protected_unless_explicitly_allowed` |
| Brak uprawnień, brak miejsca, konflikty nazw, przerwanie procesu | ✅ testy (chmod/read-only, StorageFull, `/dev/full`), sprzątanie `.imago-*.tmp` |
| Kontrolowane błędy zamiast panic | ✅ `catch_unwind` per plik + osłony parserów (ADR-004, ADR-013) |
| Brak telemetrii, sieci, uruchamiania poleceń | ✅ CSP, `core:default`, brak kodu sieciowego |
| Brak `unsafe` | ✅ `unsafe_code = "forbid"` w workspace |

## Testy

| Rodzaj | Status | Gdzie |
|---|---|---|
| Jednostkowe | ✅ | moduły `core` (121), `tauri/state.rs`, `cli` (8) |
| Integracyjne | ✅ | `core/tests`: codecs (19), engine (10), pipeline (21), scan (8), security (7) |
| CLI | ✅ | `crates/imago-cli/tests/cli.rs` (11) |
| Frontend | ✅ | Vitest (27): format, pliki, reduktor, i18n, pytania wstępne, bufor zdarzeń, App |
| Per format, błędne i uszkodzone pliki | ✅ | codecs, engine, security |
| Przezroczystość, EXIF, orientacja, animacje | ✅ | codecs, engine, pipeline |
| Skalowanie, jakość, bezstratny, auto | ✅ | resize, codecs, pipeline |
| Anulowanie, wielu workerów, zapis atomowy, konflikty | ✅ | pipeline, workers, safe_write |
| Unicode, duże pliki, brak uprawnień, brak miejsca | ✅ | engine, pipeline, safe_write, security |
| Auto nie zwiększa pliku, wymiary, ochrona oryginału | ✅ | pipeline |
| Benchmarki Criterion | ✅ | `core/benches` — [PERFORMANCE.md](PERFORMANCE.md) |

## Kryteria akceptacji

| Kryterium | Status |
|---|---|
| `npm run tauri dev` uruchamia aplikację „Imago” | ✅ |
| Przeciąganie, wybór plików i folderów | ✅ |
| Wiele plików, format, jakość, auto, skalowanie, metadane, folder wyjściowy | ✅ |
| Postęp, anulowanie, ponowienie, pobieranie wyników, ZIP | ✅ |
| Oryginały chronione, błędne pliki obsłużone, brak zawieszeń przy dużych paczkach | ✅ |
| CLI działa | ✅ |
| Testy, clippy (`-D warnings`), rustfmt | ✅ |
| Build release | ✅ lokalnie Windows (MSI + NSIS); macOS/Linux w `release.yml` |
| Dokumentacja | ✅ |
