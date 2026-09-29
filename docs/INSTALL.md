# Instalacja i budowanie

## Wymagania systemowe (gotowa aplikacja)

| System | Minimum | Uwagi |
|---|---|---|
| Windows | 10 (1809+) / 11, x64 | Wymaga WebView2 (preinstalowany w Windows 11; instalator doinstaluje go w razie potrzeby). |
| macOS | 10.15+ (Intel i Apple Silicon) | Aplikacja niepodpisana — przy pierwszym uruchomieniu: prawy klik → Otwórz. |
| Linux | glibc 2.35+ (np. Ubuntu 22.04+) | WebKitGTK 4.1. Pakiety `.deb`, `.rpm` i `.AppImage`. |

Pamięć: 4 GB RAM wystarcza; domyślny budżet dla jednocześnie przetwarzanych obrazów to 2 GB
(zmienny w ustawieniach zaawansowanych). Obrazy są przetwarzane lokalnie — nie jest potrzebny internet.

## Instalacja z gotowych pakietów

1. Pobierz plik z zakładki *Releases*: `Imago_<wersja>_x64-setup.exe` lub `Imago_<wersja>_x64_en-US.msi`
   (Windows), `Imago_<wersja>_aarch64.dmg` / `_x64.dmg` (macOS), `Imago_<wersja>_amd64.deb` / `.rpm` / `.AppImage` (Linux).
2. Zainstaluj standardowo. Instalator NSIS dla Windows instaluje aplikację dla bieżącego użytkownika (bez uprawnień
   administratora).
3. CLI: rozpakuj `imago-cli-<wersja>-<platforma>` i dodaj katalog z plikiem `imago` do `PATH`.

## Narzędzia do budowania ze źródeł

| Narzędzie | Wersja | Po co |
|---|---|---|
| Rust (rustup) | stable ≥ 1.90 | backend, CLI |
| Node.js + npm | Node 20.19+ lub 22+ | interfejs (Vite, React, TypeScript) |
| Kompilator C/C++ | MSVC Build Tools / Xcode CLT / gcc | kodeki MozJPEG, libwebp, libdeflate |
| NASM | 2.15+ | asemblerowe przyspieszenia MozJPEG, rav1e (AVIF), rav1d |

NASM i kompilator C są potrzebne **tylko do budowania** — gotowa aplikacja jest linkowana statycznie.

### Windows

```powershell
winget install Rustlang.Rustup
winget install OpenJS.NodeJS.LTS
winget install NASM.NASM
# Microsoft C++ Build Tools (składnik „Programowanie aplikacji klasycznych w języku C++”)
winget install Microsoft.VisualStudio.2022.BuildTools
```

Instalator NASM z winget może nie dopisać katalogu do `PATH`. Sprawdź `nasm -v`; jeśli nie działa, dodaj
`%LOCALAPPDATA%\bin\NASM` (lub `C:\Program Files\NASM`) do zmiennej `PATH` użytkownika i otwórz nowy terminal.

### macOS

```bash
xcode-select --install
brew install rustup node nasm
rustup-init -y
```

### Linux (Debian / Ubuntu)

```bash
sudo apt-get update
sudo apt-get install -y build-essential nasm curl \
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libxdo-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Node.js 22 LTS np. przez nvm lub pakiety NodeSource
```

Fedora: `sudo dnf install gcc-c++ nasm webkit2gtk4.1-devel libappindicator-gtk3-devel librsvg2-devel`.

## Uruchomienie deweloperskie

```bash
npm install
npm run tauri dev
```

`tauri dev` uruchamia serwer Vite (port 1420) i aplikację w trybie debug z przeładowywaniem interfejsu.
Kodeki są zawsze kompilowane z optymalizacjami (`[profile.dev.package."*"] opt-level = 3`), więc przetwarzanie
w trybie deweloperskim jest tylko nieznacznie wolniejsze. Pierwsza kompilacja trwa kilka minut.

Samo CLI:

```bash
cargo run -p imago-cli -- ./zdjecia --format webp
```

## Kontrola jakości

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
npm run typecheck
npm test
cargo bench -p imago-core            # benchmarki (kilka minut)
```

## Build release

```bash
npm run tauri build                  # instalatory aplikacji
cargo build --release -p imago-cli   # CLI: target/release/imago(.exe)
```

Wyniki `tauri build` trafiają do `target/release/bundle/`:

| Platforma | Pliki |
|---|---|
| Windows | `nsis/Imago_<wersja>_x64-setup.exe`, `msi/Imago_<wersja>_x64_en-US.msi` |
| macOS | `macos/Imago.app`, `dmg/Imago_<wersja>_<arch>.dmg` |
| Linux | `deb/Imago_<wersja>_amd64.deb`, `rpm/Imago-<wersja>-1.x86_64.rpm`, `appimage/Imago_<wersja>_amd64.AppImage` |

Przy pierwszym buildzie na Windows `tauri build` pobiera narzędzia WiX i NSIS (tylko na potrzeby budowania).

### Różnice: build deweloperski vs release

| | `tauri dev` / `cargo build` | `tauri build` / `cargo build --release` |
|---|---|---|
| Nasz kod | bez optymalizacji, z symbolami debug | `opt-level = 3`, `lto = "thin"`, `codegen-units = 1`, `strip = true` |
| Kodeki | zoptymalizowane | zoptymalizowane |
| Konsola na Windows | widoczna (logi) | ukryta |
| Logi | konsola + plik | plik |
| `panic` | `unwind` | `unwind` (celowo, ADR-004) |

### Funkcje opcjonalne (cargo features)

`imago-core` ma dwa domyślnie włączone moduły: `avif` (koder rav1e i dekoder rav1d) oraz `jxl` (JPEG XL). Build bez
nich — np. gdy NASM jest niedostępny dla AVIF — działa i czytelnie raportuje brak formatu:

```bash
cargo build --release -p imago-cli --no-default-features          # bez AVIF i JPEG XL
cargo build --release -p imago-cli --no-default-features --features jxl
```

## Lokalizacje plików aplikacji

| | Windows | macOS | Linux |
|---|---|---|---|
| Ustawienia | `%APPDATA%\imago\Imago\config\config.json` | `~/Library/Application Support/dev.imago.Imago/config.json` | `~/.config/imago/config.json` |
| Logi | `%LOCALAPPDATA%\imago\Imago\data\logs` | `~/Library/Application Support/dev.imago.Imago/logs` | `~/.local/share/imago/logs` |

Folder logów można otworzyć z ustawień zaawansowanych. Usunięcie pliku `config.json` przywraca ustawienia domyślne.
