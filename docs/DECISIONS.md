# Imago — rejestr decyzji architektonicznych (ADR)

Każda istotna decyzja lub odstępstwo od [SPEC.md](SPEC.md) jest tu zapisana z kontekstem i konsekwencjami.
Nowe decyzje dopisuj na końcu, nie zmieniaj numeracji.

---

## ADR-001 — Statycznie linkowane kodeki zamiast libvips

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** Specyfikacja preferuje libvips, „jeśli zapewni najlepszą wydajność i stabilność”, z `image` jako fallbackiem.
- **Decyzja:** Silnik oparty o `image` (dekodowanie) + wyspecjalizowane kodeki linkowane statycznie: mozjpeg, libwebp, rav1e/ravif, rav1d/avif-decode, oxipng, libimagequant, jxl-oxide. Bez libvips.
- **Uzasadnienie:**
  - libvips jest zależnością runtime (dziesiątki DLL/dylib/so do dystrybucji, podpisywanie na macOS, różne wersje w dystrybucjach Linux) — ryzyko dla stabilności i instalatorów;
  - bindingi `libvips` wymagają nagłówków, pkg-config, globalnej inicjalizacji; więcej `unsafe` na granicy;
  - te same enkodery, których używa libvips, linkujemy bezpośrednio — różnica wydajności dotyczy głównie obrazów gigapikselowych, które i tak ograniczamy limitami;
  - build jest powtarzalny w CI na trzech platformach, a gotowa aplikacja nie ma zależności runtime.
- **Konsekwencje:** wymagany kompilator C i NASM w czasie budowania (nie w runtime). Backend libvips może zostać dodany później jako opcjonalny feature za warstwą `image_engine`.

## ADR-002 — Licencja GPL-3.0-or-later i libimagequant

- **Data:** 2026-09-29 · **Status:** przyjęta (decyzja właściciela projektu)
- **Kontekst:** Jakość stratnego PNG (główna funkcja narzędzi typu TinyPNG) zależy od kwantyzatora palety. Najlepszy — libimagequant (silnik pngquant) — jest na GPL-3.0-or-later. Alternatywy permisywne (exoquant, NeuQuant) dają wyraźnie gorsze wyniki, a `quantette` nie obsługuje kanału alfa.
- **Decyzja:** Imago jest licencjonowane na **GPL-3.0-or-later** i używa `imagequant` do stratnego PNG i palet GIF.
- **Konsekwencje:** wszystkie zależności muszą być zgodne z GPL-3 (MIT, Apache-2.0, BSD, Zlib, IJG, MPL-2.0 — zgodne). Weryfikacja licencji w CI (`cargo deny`). Lista licencji zależności w `THIRD_PARTY.md`.

## ADR-003 — Workspace: `imago-core` + `imago-cli` + `src-tauri`

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** Specyfikacja proponuje wszystkie moduły w `src-tauri/src`, ale jednocześnie wymaga crate'a `imago-core`, oddzielenia CLI i Tauri od silnika oraz wspólnych struktur danych.
- **Decyzja:** Cargo workspace z trzema pakietami: `crates/imago-core` (biblioteka, bez Tauri), `crates/imago-cli` (binarka `imago`), `src-tauri` (pakiet `imago-app`, adapter GUI).
- **Konsekwencje:** separacja wymuszona przez kompilator; CLI budowane bez WebView; testy i benchmarki silnika w `imago-core`. Binarka CLI dystrybuowana osobno. Plik wykonywalny GUI nazywa się `imago-app` (nie `Imago`): na systemach plików bez rozróżniania wielkości liter (Windows, domyślnie macOS) `Imago.exe` i `imago.exe` to ten sam plik — wcześniejsze `mainBinaryName: "Imago"` powodowało nadpisanie binarki GUI przez CLI w `target/release`. Nazwa aplikacji, skróty i instalatory pozostają „Imago”.

## ADR-004 — `panic = "unwind"` w profilu release zamiast `"abort"`

- **Data:** 2026-09-29 · **Status:** przyjęta (odstępstwo od SPEC)
- **Kontekst:** Specyfikacja sugeruje `panic = "abort"`, dopuszczając bezpieczną alternatywę, jeśli ustawienie jest niekompatybilne. Jednocześnie wymaga „kontrolowanych błędów zamiast panic” i stabilności przy uszkodzonych plikach.
- **Decyzja:** `panic = "unwind"`. Każdy plik jest przetwarzany wewnątrz `catch_unwind`.
- **Uzasadnienie:** dekodery przetwarzają niezaufane dane; ewentualna panika w zależności (błąd w bibliotece, złośliwy plik) przy `abort` zabiłaby cały proces — w GUI utratę stanu całej paczki. Dodatkowo `mozjpeg` raportuje błędy libjpeg przez mechanizm unwinding — z `abort` każdy błąd kodera kończyłby proces.
- **Konsekwencje:** minimalnie większa binarka. Pozostałe ustawienia (`opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = true`) bez zmian.

## ADR-005 — Własna pula wątków workerów zamiast `rayon` na poziomie plików

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** N wątków `std::thread::scope` pobierających zadania z kolejki `crossbeam-channel`; rayon wyłącznie wewnątrz kodeków i do równoległego skanowania.
- **Uzasadnienie:** workerzy blokują się na budżecie pamięci; blokowanie w puli rayon zagłodziłoby kodeki używające tej samej puli. Własna pula daje też prosty model anulowania i pomijania plików.

## ADR-006 — Wynik większy od oryginału

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** polityka `when_larger`:
  - `keep_original` (domyślna) — gdy żaden kandydat nie jest mniejszy, a format się nie zmienia (lub tryb auto), do wyjścia trafia oryginał (bezstratnie oczyszczony z metadanych, jeśli użytkownik tego chce i nie zmienia to wyświetlania, np. orientacji);
  - `write_anyway` — „wymuszenie konwersji”;
  - przy jawnie wybranym innym formacie wynik jest zapisywany zawsze (użytkownik chce konwersji), z ostrzeżeniem, jeśli urósł.
- **Konsekwencje:** folder wynikowy jest kompletny (każdy plik wejściowy ma odpowiednik), a tryb auto nigdy nie zwiększa rozmiaru.

## ADR-007 — Profil ICC

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** profil sRGB jest zawsze usuwany (brak zmiany wyglądu, oszczędność ~3 KB); profil inny niż sRGB jest zachowywany przy `keep_icc = true` (domyślnie), a przy `keep_icc = false` piksele są najpierw konwertowane do sRGB (`moxcms`), potem profil usuwany.
- **Uzasadnienie:** priorytet 1 — usunięcie profilu Display P3/AdobeRGB bez konwersji zmienia kolory.

## ADR-008 — Rozszerzenie pliku a format w CLI

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** Przykład ze specyfikacji: `imago input.jpg --output optimized.jpg --format webp`.
- **Decyzja:** jeśli podano jawnie `--format` i rozszerzenie pliku wyjściowego do niego nie pasuje, rozszerzenie jest poprawiane (`optimized.webp`) i wypisywane jest ostrzeżenie. Bez `--format` format wynika z rozszerzenia pliku wyjściowego.
- **Uzasadnienie:** zapis danych WebP do pliku `.jpg` byłby niepoprawnym wynikiem (priorytet 1).

## ADR-009 — Narzędzia build-time: NASM i Rust ≥ 1.90

- **Data:** 2026-09-29 · **Status:** przyjęta (zgoda właściciela; NASM 3.02 zainstalowany przez winget, Rust zaktualizowany do 1.98.1)
- **Decyzja:** NASM wymagany do budowania (SIMD mozjpeg, asm rav1e/rav1d); minimalna wersja Rust wynika z zależności.
- **Konsekwencje:** instrukcje instalacji per platforma; CI instaluje NASM.

## ADR-010 — Styl frontendu: CSS z custom properties, stan: zustand

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** zamiast Tailwind — nowoczesny CSS (custom properties, `color-scheme`, `prefers-color-scheme`, `prefers-reduced-motion`). Stan w `zustand` (mały, bez boilerplate). Lista plików wirtualizowana własnym komponentem (bez dodatkowej zależności).
- **Uzasadnienie:** mniej zależności, pełna kontrola nad motywami jasnym/ciemnym i dostępnością.

## ADR-011 — Miniatury i podgląd generowane przez backend

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** zamiast protokołu `asset://` z szerokim zakresem systemu plików, backend generuje miniatury/podglądy (PNG/JPEG) wyłącznie dla plików zarejestrowanych w bieżącej sesji (dodanych przez użytkownika lub wyników).
- **Uzasadnienie:** mniejsza powierzchnia ataku (webview nie czyta dowolnych plików), obsługa formatów, których WebView nie wyświetla (TIFF, JXL, AVIF na części platform), kontrola pamięci.

## ADR-012 — Własny muxer animowanego WebP

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** `webp::AnimEncoder` (crate `webp` 0.3.1) trzyma wszystkie klatki w pamięci do końca, ignoruje wynik końcowego `WebPAnimEncoderAdd` (przekazuje znacznik czasu 0, co w libwebp jest błędem przy >1 klatce) i wywołuje `unwrap()`. Alternatywa `webp-animation` używa drugiej kopii libwebp (`libwebp-sys2`) — ryzyko konfliktu symboli przy linkowaniu statycznym.
- **Decyzja:** każda klatka jest kodowana bezpiecznym API do statycznego WebP, a `codecs/webp_mux.rs` składa kontener (VP8X + ANIM + ANMF) zgodnie ze specyfikacją kontenera WebP. Kodowany jest tylko prostokąt zmian względem poprzedniej klatki; identyczne klatki wydłużają czas poprzedniej.
- **Konsekwencje:** przetwarzanie strumieniowe (w pamięci: poprzednie płótno + jedna zakodowana klatka), zero `unsafe` w projekcie. Kompresja nieco słabsza niż WebPAnimEncoder (brak prób blend/no-blend per klatka) — do rozważenia w przyszłości.

## ADR-013 — Izolacja paniki także w sondowaniu i dekodowaniu

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** test uszkodzonych plików wykazał, że `avif-parse` kończy się `panic!` (asercja stanu parsera) na obciętym pliku.
- **Decyzja:** `probe_*` i `decode` uruchamiają kod zewnętrznych parserów przez `guard_decoder` (`catch_unwind`), a `codecs::encode`/`encode_animation` — przez analogiczną osłonę. Panika = `decode_failed`/`encode_failed` dla jednego pliku.

## ADR-014 — Dialogi plików po stronie Rusta i rejestr ścieżek sesji

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Decyzja:** okna „Dodaj pliki/folder”, „Wybierz folder wyjściowy”, „Zapisz jako…” i zapis ZIP są otwierane przez
  komendy Rusta (`tauri-plugin-dialog`, wywołania blokujące w `spawn_blocking`). Backend prowadzi rejestr ścieżek
  (`PathRegistry`): wejścia dodane przez użytkownika i wyniki zapisane w sesji. Miniatury, podgląd, eksport,
  „pokaż w folderze” i start zadania akceptują tylko ścieżki z rejestru.
- **Uzasadnienie:** webview nie potrzebuje żadnych uprawnień poza `core:default`; nawet przejęty interfejs nie mógłby
  odczytać dowolnego pliku ani wskazać celu zapisu.
- **Konsekwencje:** przeciąganie plików przechodzi przez `scan_paths`, które rejestruje znalezione obrazy.

## ADR-015 — Dokładny plik wyjściowy (`OutputLocation::File`)

- **Data:** 2026-09-29 · **Status:** przyjęta
- **Kontekst:** `imago input.jpg --output optimized.jpg` wymaga zapisu pod konkretną nazwą, a nie w folderze.
- **Decyzja:** nowy wariant lokalizacji wyjścia `File { path }`, dozwolony tylko dla zadań jednoplikowych. Rozszerzenie
  jest poprawiane, gdy nie pasuje do formatu (ADR-008), z ostrzeżeniem `output_extension_changed`.
