# Imago — postęp prac

> Żywy dokument. Aktualizowany po każdym etapie i każdej sesji pracy.
> Wymagania: [SPEC.md](SPEC.md) · Architektura: [ARCHITECTURE.md](ARCHITECTURE.md) ·
> Decyzje: [DECISIONS.md](DECISIONS.md) · Macierz wymagań: [REQUIREMENTS.md](REQUIREMENTS.md)

Legenda: ✅ zrobione · 🟡 w toku / częściowo · ⬜ do zrobienia · ⛔ zablokowane

## Podsumowanie etapów

| Etap | Zakres | Status |
|---|---|---|
| 1 | Architektura | ✅ |
| 2 | Minimalny projekt (workspace, Tauri, React, TS) | ✅ |
| 3 | Silnik obrazów | ✅ |
| 4 | Pipeline i wielowątkowość | ✅ |
| 5 | Interfejs | ✅ |
| 6 | Funkcje zaawansowane | ✅ |
| 7 | CLI | ✅ |
| 8 | Testy i release | ✅ |

---

## Etap 1 — Architektura ✅

- ✅ Analiza wymagań, zapis specyfikacji w `docs/SPEC.md`
- ✅ Architektura warstwowa, struktura katalogów — `docs/ARCHITECTURE.md`
- ✅ Wybór bibliotek i uzasadnienie (libvips vs statyczne kodeki) — ADR-001
- ✅ Licencja: GPL-3.0-or-later + libimagequant — ADR-002
- ✅ Wielowątkowość, anulowanie, bezpieczny zapis, duże pliki — ARCHITECTURE §7–10
- ✅ Ryzyka techniczne — ARCHITECTURE §14
- ✅ Środowisko: Rust 1.98.1, NASM 3.02 (w PATH użytkownika), Node 23.8, VS Build Tools 2022

## Etap 2 — Minimalny projekt ✅

- ✅ Cargo workspace (`imago-core`, `imago-cli`, `src-tauri`), profile dev/release (`panic = "unwind"`, ADR-004)
- ✅ `imago-core`: szkielet biblioteki, `errors` (kompletny `ImagoError` z kodami), `app_info()`
- ✅ `imago-cli`: binarka `imago --version/--help`
- ✅ Frontend: Vite 8 + React 19 + TypeScript 7, Vitest 5
- ✅ Tauri 2.12: `tauri.conf.json` (productName **Imago**, okno „Imago — Image Optimizer”), capabilities (core + dialog), ikony z SVG, CSP bez sieci
- ✅ `cargo build`, `cargo clippy -D warnings`, `cargo fmt --check`, `cargo test`, `npm run typecheck`, `vite build`, `npm test` — zielone
- ✅ `npm run tauri dev` uruchamia okno „Imago” — zweryfikowane przez UI Automation: UI wyświetla wersję pobraną z Rusta przez IPC
- ✅ `.gitignore`, `.gitattributes`, `.editorconfig`, `rustfmt.toml`, `clippy.toml`, `LICENSE` (GPL-3.0, kanoniczny tekst)
- Uwaga: npm 11.3 ma błąd arborist przy pierwszej instalacji (peer set `@vitest/mocker`); lockfile wygenerowano npm 12, z lockfile npm 11.3 działa. Zrezygnowano z `@testing-library/jest-dom`.

## Etap 3 — Silnik obrazów ✅

- ✅ Wykrywanie formatu po sygnaturze (`image_engine/detection.rs`) — JPEG, PNG, GIF, WebP, TIFF/BigTIFF, BMP (walidacja nagłówka DIB), JXL, AVIF (marki `ftyp`), rozpoznawanie HEIC/PDF/PSD/SVG/ICO jako nieobsługiwanych
- ✅ Sondowanie nagłówków bez dekodowania (`probe.rs`): wymiary, alfa, animacja (GIF — liczenie klatek, WebP — flaga ANIM, APNG, AVIF `avis`), głębia bitowa, ICC, EXIF, orientacja; limity rozmiaru/wymiarów/pikseli **przed** alokacją
- ✅ Dekodowanie z limitami (`decode.rs`) — wszystkie 8 formatów wejściowych; ICC z kontenera jako fallback
- ✅ Izolacja paniki dekoderów (`guard_decoder`) — wykryte w testach: `avif-parse` panikuje na uszkodzonym pliku → kontrolowany błąd
- ✅ Kodeki (`codecs/`): JPEG (mozjpeg), PNG (imagequant + oxipng `RawImage`), WebP (libwebp), AVIF (ravif), TIFF (Deflate+predyktor), GIF (imagequant), JPEG XL (bezstratny, weryfikacja round-trip)
- ✅ Animacje: własny muxer RIFF dla WebP (ADR-012), kodowanie tylko zmienionych prostokątów, łączenie identycznych klatek; GIF z obsługą przezroczystości (dispose)
- ✅ Rejestr możliwości formatów (`capabilities.rs`) z alternatywą i instrukcją włączenia
- ✅ Skalowanie (`resize.rs`, fast_image_resize): szerokość, wysokość, proporcje, dopasowanie do maks., procent, blokada powiększania; alfa premultiplikowana
- ✅ Kolor (`color.rs`): analiza alfy, liczenie kolorów, spłaszczanie na białe tło, klasyfikacja ICC (sRGB/inne/nieużywalne), konwersja do sRGB (moxcms)
- ✅ Metadane (`metadata.rs`): bezstratne usuwanie EXIF/XMP/komentarzy z JPEG/PNG/WebP, reset orientacji EXIF
- ✅ Token anulowania (`cancel.rs`) z hierarchią zadanie → plik
- ✅ Testy: 69 jednostkowych + 19 kodeków + 10 silnika; build i testy także z `--no-default-features`

## Etap 4 — Pipeline i wielowątkowość ✅

- ✅ Model zadania, statusy (Oczekuje/Przetwarzanie/Ukończono/Pominięto/Anulowano/Błąd), etapy, zdarzenia, raport pliku, podsumowanie (`jobs.rs`)
- ✅ Token anulowania (zadanie + plik) — „pomiń plik” ≠ „anuluj wszystko”
- ✅ Pula workerów (`std::thread::scope` + `crossbeam-channel`), domyślnie rdzenie−1 (`workers.rs`)
- ✅ Budżet pamięci w bajtach (semafor z Condvar, obraz większy niż budżet przetwarzany sam, oczekiwanie przerywalne)
- ✅ Pipeline 23 kroków z pomiarem czasów i logowaniem `tracing` (`image_engine/pipeline.rs`)
- ✅ Tryb automatyczny: klasyfikacja zdjęcie/grafika/ikona, kandydaci, najmniejszy wygrywa, oryginał (bezstratnie oczyszczony) gdy nic nie jest mniejsze (`auto.rs`)
- ✅ Animacje: strumieniowe dekodowanie klatek (GIF/WebP/APNG), limit łącznej liczby pikseli, zachowanie domyślne, pierwsza klatka tylko za zgodą (`animation.rs`)
- ✅ Przezroczystość: nigdy nie usuwana bez zgody (fallback do formatu z alfą + ostrzeżenie), spłaszczanie tylko przy `FlattenWhite`
- ✅ ICC: sRGB usuwany, inne zachowane lub konwertowane do sRGB, gdy format nie obsługuje ICC
- ✅ Bezpieczny zapis atomowy: tmp w katalogu docelowym → fsync → weryfikacja (odczyt bajt-w-bajt + sygnatura) → `persist_noclobber`/`persist` (`filesystem/safe_write.rs`)
- ✅ Sprzątanie osieroconych `.imago-*.tmp` (>1 h) raz na katalog w zadaniu
- ✅ Ścieżki wyjściowe: `optimized`, folder wybrany, ten sam folder, przyrostek, struktura, zachowanie pisowni rozszerzenia, ochrona przed path traversal (`filesystem/paths.rs`)
- ✅ Konflikty nazw: zmień nazwę `(1)`, pomiń, nadpisz; rejestr nazw w zadaniu (dwa wejścia → ta sama nazwa nigdy się nie nadpisują); ochrona oryginału (`same-file`)
- ✅ Skanowanie folderów: rekurencja, ukryte pliki (kropka + atrybut Windows), wykluczenie `optimized`, wykrywanie po treści, deduplikacja, bez podążania za dowiązaniami do katalogów (`filesystem/scan.rs`)
- ✅ Izolacja paniki per plik (`catch_unwind`)
- ✅ Testy: anulowanie, pomijanie, wielu workerów (24 pliki / 4 wątki), zapis atomowy, brak miejsca (symulacja + `/dev/full` na Linux), konflikty, Unicode, brak uprawnień (Unix chmod / Windows read-only), ochrona oryginału, zachowanie daty — razem 160 testów

## Etap 5 — Interfejs ✅

- ✅ Komendy Tauri (`src-tauri/src/commands.rs`): informacje/możliwości, konfiguracja, dialogi (po stronie Rusta, ADR-014), skanowanie z opisem plików, zadania ze strumieniem zdarzeń (`Channel`), anulowanie/pomijanie, miniatury i podgląd (binarnie), pokaż w folderze, otwórz folder, eksport, ZIP
- ✅ Rejestr ścieżek sesji — webview ma wyłącznie `core:default`
- ✅ Dropzone (natywne przeciąganie plików i folderów), przyciski „Dodaj pliki” / „Dodaj folder”
- ✅ Lista wirtualizowana (stała wysokość wiersza) z miniaturą, nazwą, formatami, wymiarami, rozmiarami, redukcją, czasem, statusem, błędem/ostrzeżeniem/decyzją, akcjami (podgląd, pokaż w folderze, zapisz jako, ponów, pomiń, usuń)
- ✅ Panel ustawień: format (niedostępne wyszarzone z powodem), jakość, bezstratny, maks. kompresja, skalowanie, metadane, zapis, zaawansowane (wątki, rekurencja, ukryte, wymuszenie, animacje, przezroczystość, tryb szybki, limity, fsync, nadpisywanie oryginałów, logi)
- ✅ Pasek postępu globalny i per plik (etap), podsumowanie (przetworzone, pominięte, błędy, oszczędność, czas)
- ✅ Pytania przed startem: nadpisywanie, animacje + format bez animacji, przezroczystość + JPEG
- ✅ Podgląd przed/po z suwakiem porównania
- ✅ Motyw systemowy/jasny/ciemny, PL/EN (wykrywanie języka systemu), skróty klawiszowe, sortowanie, filtrowanie, powiadomienia
- ✅ Grupowanie zdarzeń (~12 aktualizacji/s), miniatury leniwie po 3 naraz z pamięcią podręczną
- ✅ Testy Vitest (27) + weryfikacja end-to-end w prawdziwym oknie przez UI Automation: dodanie folderu natywnym dialogiem, optymalizacja 4 plików (−95%), podgląd, motyw jasny

## Etap 6 — Funkcje zaawansowane ✅

- ✅ Tryb automatyczny (zrealizowany w Etapie 4) z uzasadnieniem decyzji PL/EN
- ✅ Metadane: EXIF, ICC (zachowanie / sRGB), daty, bezstratne czyszczenie kontenera (Etapy 3–4)
- ✅ ZIP (`core/archive.rs`): strumieniowo, Stored, ZIP64, unikalne i bezpieczne nazwy, zapis atomowy z weryfikacją
- ✅ Konfiguracja (`core/config.rs`): wersjonowany JSON, zapis atomowy, naprawa nieprawidłowych wartości, brak historii plików
- ✅ Konflikty nazw i animacje (Etap 4)
- ✅ Logowanie do pliku z rotacją dzienną (7 plików) + panika w logu (`src-tauri/src/logging.rs`)
- ✅ Podgląd/miniatury (`core/preview.rs`), eksport pojedynczego wyniku (`core/app.rs::export_file`)

## Etap 7 — CLI ✅

- ✅ `imago` z pełnym zestawem opcji ze specyfikacji (`crates/imago-cli/src/args.rs`) — ten sam silnik i `OptimizeSettings`
- ✅ Dokładny plik wyjściowy z korektą rozszerzenia (ADR-008, ADR-015)
- ✅ Pasek postępu (tylko w terminalu), linia na plik, podsumowanie, `-q`, `-v`/`-vv`, `--json`, `--list-formats`
- ✅ Kody wyjścia 0/1/2/3/130, Ctrl+C (łagodnie, drugi raz natychmiast)
- ✅ Testy CLI (11 × assert_cmd) + 8 jednostkowych

## Etap 8 — Testy i release ✅

- ✅ Benchmarki Criterion (`core/benches/codecs.rs`, `pipeline.rs`): kodowanie, dekodowanie, jakość, skalowanie, konwersje, pojedynczy plik, paczka 1–16 wątków — wyniki w `docs/PERFORMANCE.md`
- ✅ Testy bezpieczeństwa (`core/tests/security.rs`): losowe dane za sygnaturami, odwrócone bajty nagłówków, wrogie nazwy ZIP, ucieczka ścieżek, bomby, pętle dowiązań
- ✅ GitHub Actions: `ci.yml` (fmt, clippy, testy na 3 OS, frontend, build bez opcjonalnych kodeków, kompilacja benchmarków), `release.yml` (instalatory przez tauri-action + archiwa CLI), `audit.yml` (cargo-deny, npm audit)
- ✅ `deny.toml`, `THIRD_PARTY.md` (562 pakiety Rust + 5 JS, wszystkie licencje zgodne z GPL-3.0)
- ✅ Dokumentacja: README, CONTRIBUTING, SECURITY, docs/INSTALL, CLI, FORMATS, PIPELINE, PERFORMANCE, LIMITATIONS, TROUBLESHOOTING, ARCHITECTURE (zaktualizowana), DECISIONS (ADR-001…015)
- ✅ `npm run tauri build` — instalatory Windows (MSI + NSIS) zbudowane lokalnie; macOS/Linux w `release.yml`

---

## Dziennik sesji

### 2026-09-29 — sesja 1
- Etap 1 ukończony: dokumenty `SPEC`, `ARCHITECTURE`, `DECISIONS`, `PROGRESS`, `REQUIREMENTS`.
- Zainstalowano NASM 3.02 (winget, katalog dopisany do PATH użytkownika), Rust 1.89 → 1.98.1.
- Decyzje właściciela: licencja GPL-3.0 + libimagequant; zgoda na narzędzia build-time.
- Etap 2 ukończony (okno Imago zweryfikowane przez UI Automation).
- Etap 3 ukończony (silnik obrazów, 98 testów).
- Etap 4 ukończony (pipeline, workerzy, bezpieczny zapis; 160 testów). Tryb auto i animacje zrealizowane już w Etapie 4, bo są częścią pipeline'u.
- Etapy 5–8 ukończone: interfejs (weryfikacja E2E w prawdziwym oknie), ZIP, konfiguracja, logi, pełne CLI, benchmarki,
  testy bezpieczeństwa, CI, dokumentacja, build release. Testy: 206 Rust + 27 frontend.

### 2026-09-30 — sesja 2: odświeżenie wyglądu
- Panel ustawień jako zwijane karty z ikoną i jednolinijkowym podsumowaniem (`utils/settingsSummary.ts`);
  domyślnie otwarte „Format” i „Jakość”, stan „zaawansowanych” zapisywany w `config.ui.showAdvanced`.
  Karta blokuje tylko pola (fieldset) w trakcie zadania — zwijanie działa nadal.
- Wybór formatu: dwa kafelki trybu („Automatycznie”, „Zachowaj format”) + „Albo wybierz format” (6 formatów).
- Jakość: gotowe ustawienia (60 / 82 / 92), suwak z opisem skali, pole liczbowe; checkboxy → przełączniki (`role="switch"`).
- Lista plików: wiersz 84 px, rozmiary „przed → po” z odznaką redukcji, status jako kolorowa pigułka,
  dyskretne akcje (pełna widoczność przy najechaniu/fokusie), notatka z wielokropkiem.
- Stopka: pasek postępu na górnej krawędzi, wyróżnione „Zaoszczędzono”, statystyki w jednej linii.
- Pusty stan: trzy wyróżniki (lokalnie, bezpieczne oryginały, automatyczny wybór formatu).
- Tokeny: większe fonty (13 → 14 px dla tekstu pomocniczego), lepszy kontrast `text-muted`/`text-subtle`, szerszy panel (392 px).
- Weryfikacja: typecheck, 34 testy frontendu, build Vite, zrzuty prawdziwego okna (jasny/ciemny, pusty/po zadaniu).
- Poprawka po uwagach właściciela („menu z wyborami zbyt brzydkie”): obramowane kafelki zastąpione dwoma wzorcami —
  kontrolka segmentowa (wgłębiony tor, wybrana opcja „uniesiona”; formaty i gotowe ustawienia jakości) oraz lista
  opcji z kropką radia (tryb formatu, skalowanie, miejsce zapisu). Listy rozwijane mają własną strzałkę w kolorach motywu.
  Nowe tokeny `--track`, `--track-hover`, `--raised`.

## Możliwe dalsze kroki

- Podpisywanie instalatorów (Windows Authenticode, macOS notarization) — wymaga certyfikatów właściciela.
- Stratny JPEG XL (libjxl przez CMake) jako opcjonalny feature.
- Zapis APNG i animowanego AVIF.
- Optymalizacja animowanego WebP metodą WebPAnimEncoder (blend/no-blend per klatka) bez `unsafe` w projekcie.
- Opcjonalny backend libvips dla obrazów gigapikselowych (ADR-001).
