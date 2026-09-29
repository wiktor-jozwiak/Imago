# Rozwiązywanie problemów

## Budowanie

**`nasm: command not found` / `NASM not found` przy kompilacji mozjpeg-sys, rav1e lub rav1d**
Zainstaluj NASM (zob. [INSTALL.md](INSTALL.md)) i upewnij się, że `nasm -v` działa w nowym terminalu. Na Windows
winget instaluje NASM do `%LOCALAPPDATA%\bin\NASM` i nie zawsze dopisuje go do `PATH`. Awaryjnie możesz zbudować
bez AVIF: `cargo build -p imago-cli --no-default-features --features jxl`.

**`link.exe not found` / `cl.exe` (Windows)**
Zainstaluj Microsoft C++ Build Tools ze składnikiem „Programowanie aplikacji klasycznych w języku C++”.

**`webkit2gtk-4.1` / `javascriptcoregtk` not found (Linux)**
Zainstaluj pakiety `libwebkit2gtk-4.1-dev` (Debian/Ubuntu) lub `webkit2gtk4.1-devel` (Fedora).

**`npm install` kończy się błędem `Cannot read properties of null (reading 'edgesOut')`**
To błąd starszych wersji npm (11.3) przy instalacji bez `package-lock.json`. Repozytorium zawiera lockfile —
użyj `npm ci` albo zaktualizuj npm/Node do wersji LTS.

**`tauri dev`: port 1420 is already in use**
Zamknij inny serwer Vite (np. poprzednie `npm run dev`) albo proces `node` używający portu 1420.

**Pierwszy build trwa bardzo długo**
Kodeki (rav1e, libwebp, MozJPEG) kompilują się z optymalizacjami także w trybie debug — pierwszy build to kilka minut,
kolejne są przyrostowe.

## Działanie aplikacji

**Plik ma status „Błąd: Nieobsługiwany format pliku”**
Zawartość pliku nie jest obrazem w obsługiwanym formacie (niezależnie od rozszerzenia) — np. HEIC z iPhone'a.
Przekonwertuj go innym narzędziem albo zapisz aparat w trybie „Najbardziej zgodny” (JPEG).

**„Nie udało się odczytać obrazu — plik może być uszkodzony”**
Plik jest obcięty lub uszkodzony. Pozostałe pliki są przetwarzane normalnie; szczegóły techniczne w dymku i w logu.

**„Obraz przekracza limit wymiarów” / „Plik przekracza ustawiony limit rozmiaru”**
Limity chronią przed wyczerpaniem pamięci. Zwiększ je w *Ustawieniach zaawansowanych → Limity bezpieczeństwa*
(CLI: `--max-megapixels`, `--max-file-size`).

**„Ochrona oryginału: nadpisanie wymaga zgody”**
Wynik miałby zastąpić plik źródłowy. Wybierz inny folder wyjściowy lub przyrostek, albo świadomie włącz
„Pozwól nadpisywać pliki oryginalne” (razem z polityką „Nadpisz”).

**Wynik jest taki sam jak oryginał („Pozostawiono oryginał”)**
Tryb automatyczny nie zapisuje większych plików. Jeśli mimo to chcesz konwersji, wybierz format ręcznie
albo ustaw „Gdy wynik jest większy → Wymuś konwersję” (CLI: `--force`).

**JPEG został zapisany jako PNG**
Obraz ma przezroczystość, a JPEG jej nie obsługuje. Aplikacja pyta o to przed startem; aby zastąpić przezroczystość
białym tłem, wybierz „Zastąp białym tłem” (CLI: `--flatten`).

**Animacja GIF pozostała GIF-em mimo wybrania JPEG**
Animacje są zachowywane domyślnie. Wybierz WebP (zachowa animację) albo „Tylko pierwsza klatka”.

**Kolory różnią się po konwersji do AVIF / GIF / JPEG XL**
Te formaty nie przechowują profilu ICC; obrazy Display P3 / Adobe RGB są konwertowane do sRGB, więc najbardziej
nasycone barwy mogą zostać przycięte do gamy sRGB. Użyj WebP, PNG lub JPEG, aby zachować profil.

**Brak miejsca na dysku**
Plik dostaje status *Błąd* („Brak miejsca na dysku”), plik tymczasowy jest usuwany, oryginał pozostaje nietknięty.
Zwolnij miejsce i użyj „Ponów nieudane”.

**Windows SmartScreen / macOS „nie można zweryfikować dewelopera”**
Instalatory nie są podpisane. Windows: *Więcej informacji → Uruchom mimo to*. macOS: prawy klik na aplikacji → *Otwórz*.

## Diagnostyka

- Logi: *Ustawienia zaawansowane → Otwórz folder logów* (lokalizacje w [INSTALL.md](INSTALL.md)).
  Poziom szczegółowości: zmienna środowiskowa `IMAGO_LOG=debug`.
- CLI: `imago -vv …` wypisuje decyzje, czasy etapów i log diagnostyczny.
- Reset ustawień: usuń `config.json` (lokalizacja w [INSTALL.md](INSTALL.md)).
- Lista formatów w danej wersji: `imago --list-formats`.
