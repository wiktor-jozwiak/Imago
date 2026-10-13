<p align="center"><img src="public/imago.svg" width="96" height="96" alt="Imago"></p>

<h1 align="center">Imago — Image Optimizer</h1>
<p align="center"><b>Fast. Local. Smaller images.</b><br>Szybsze obrazy. Mniejsze pliki. Zero chmury.</p>

<p align="center">
  <a href="README.md">🇬🇧 English</a> &nbsp;|&nbsp; <b>🇵🇱 Polski</b>
</p>

---

Imago to darmowa, natywna aplikacja desktopowa i narzędzie wiersza poleceń do **kompresji, konwersji i skalowania obrazów wyłącznie lokalnie**. Działa jak internetowe optymalizatory obrazów, ale żaden plik nie opuszcza komputera.

## Najważniejsze funkcje

- **Przeciągnij i upuść** pliki lub całe foldery (także podfoldery), albo wybierz je natywnym oknem systemu.
- **Tryb automatyczny** — dla każdego obrazu porównuje kilka formatów (WebP, AVIF, JPEG, PNG) i zapisuje najmniejszy. Zachowuje przezroczystość i animacje, **nigdy nie zwiększa pliku** i tłumaczy decyzję, np. *„Wybrano WebP, ponieważ zmniejszył rozmiar pliku o 71% i zachował przezroczystość.”*
- **Formaty wejściowe:** JPEG, PNG (z APNG), WebP, AVIF, GIF, JPEG XL (wszystkie także animowane), HEIC/HEIF (zdjęcia z iPhone'a), TIFF, BMP.
- **Formaty wyjściowe:** JPEG (MozJPEG), PNG (stratny jak TinyPNG lub bezstratny, także animowany APNG), WebP, AVIF, TIFF, JPEG XL (stratny i bezstratny). Animacje można zapisać jako WebP, APNG, AVIF, JPEG XL lub GIF.
- **Tryb dużych obrazów:** JPEG i PNG ponad limit pikseli (np. panoramy 30 000 px) są przetwarzane pasami, z pamięcią zależną od szerokości, a nie pola obrazu.
- **Jakość 1–100**, tryb bezstratny, tryb maksymalnej kompresji.
- **Skalowanie:** szerokość, wysokość, zachowanie proporcji, maksymalne wymiary, skala procentowa.
- **Metadane:** usuwanie EXIF (GPS, aparat), zachowanie profilu ICC (lub konwersja do sRGB), obrót wg EXIF, zachowanie daty modyfikacji. Metadane JPEG/PNG/WebP można usunąć bez ponownego kodowania.
- **Bezpieczny zapis:** wynik trafia najpierw do pliku tymczasowego, jest weryfikowany i atomowo przenoszony. Oryginały nigdy nie są nadpisywane bez wyraźnej zgody.
- Folder `optimized`, własny folder wyjściowy, przyrostek nazw, zachowanie struktury folderów, obsługa konfliktów nazw.
- **Podgląd przed/po** z suwakiem porównania, rozmiary, procent redukcji, czas przetwarzania.
- **ZIP** z wynikami, zapis pojedynczego wyniku („Zapisz jako…”), otwieranie folderu wynikowego.
- Pasek postępu całego zadania i każdego pliku, anulowanie, pomijanie pojedynczego pliku, ponawianie.
- Motyw jasny/ciemny/systemowy, polski i angielski interfejs, skróty klawiaturowe, sortowanie i filtrowanie.
- **CLI `imago`** z tym samym silnikiem i tymi samymi ustawieniami co aplikacja.

## Prywatność

- przetwarzanie wyłącznie lokalne, działa offline;
- brak logowania, reklam, telemetrii i zewnętrznych trackerów;
- aplikacja nie zawiera kodu sieciowego, a polityka CSP okna blokuje połączenia zewnętrzne;
- ustawienia zapisywane są lokalnie i nie zawierają historii plików; logi diagnostyczne (ścieżki i błędy, bez treści obrazów) zostają na dysku i są rotowane po 7 dniach.

## Instalacja

Gotowe instalatory (Windows `.msi`/`.exe`, macOS `.dmg`, Linux `.deb`/`.rpm`/`.AppImage`) i archiwa CLI są publikowane w zakładce *Releases*. Budowanie ze źródeł i wymagania systemowe: **[docs/INSTALL.md](docs/INSTALL.md)**.

Szybki start ze źródeł (Windows, macOS, Linux):

```bash
npm install
npm run tauri dev
```

Wymagane narzędzia: Rust stable ≥ 1.93, Node.js 20.19+ / 22+, kompilator C, **NASM** (tylko do budowania), na Linuxie biblioteki WebKitGTK. Gotowa aplikacja nie ma żadnych zależności uruchomieniowych poza WebView systemu.

## Wiersz poleceń

```bash
imago input.jpg --output optimized.jpg --format webp --quality 82
imago ./photos --output ./optimized --format auto --quality 82
imago ./photos --output ./optimized --recursive --workers 8
imago ./photos --output ./optimized --width 2400 --remove-metadata
imago ./photos --output ./optimized --format avif --quality 70
imago ./photos --output ./optimized --lossless
imago ./photos --output ./optimized --skip-existing
imago ./photos --zip photos.zip --json
imago --help
```

Pełny opis opcji i kodów wyjścia: **[docs/CLI.md](docs/CLI.md)**.

## Wydajność

- pliki są przetwarzane równolegle przez pulę wątków (domyślnie liczba rdzeni logicznych − 1);
- budżet pamięci ogranicza liczbę jednocześnie zdekodowanych dużych obrazów, a limity rozmiaru i wymiarów są sprawdzane z nagłówka **przed** dekodowaniem (ochrona przed „bombami” dekompresyjnymi);
- skalowanie SIMD (Lanczos3), kodeki natywne (MozJPEG, libwebp, rav1e, libimagequant, libdeflate) linkowane statycznie;
- animacje przetwarzane strumieniowo, klatka po klatce;
- lista w interfejsie jest wirtualizowana — tysiące plików nie spowalniają okna.

Wyniki benchmarków i sposób ich uruchamiania: **[docs/PERFORMANCE.md](docs/PERFORMANCE.md)**.

## Dokumentacja

| Dokument | Zawartość |
|---|---|
| [docs/INSTALL.md](docs/INSTALL.md) | instalacja, uruchomienie deweloperskie, build release, Windows / macOS / Linux |
| [docs/CLI.md](docs/CLI.md) | instrukcja wiersza poleceń |
| [docs/FORMATS.md](docs/FORMATS.md) | obsługiwane formaty i tryby |
| [docs/PIPELINE.md](docs/PIPELINE.md) | 23 kroki przetwarzania pliku, bezpieczny zapis |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | architektura, wielowątkowość, pamięć, anulowanie |
| [docs/LIMITATIONS.md](docs/LIMITATIONS.md) | znane ograniczenia |
| [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) | rozwiązywanie problemów |
| [docs/PERFORMANCE.md](docs/PERFORMANCE.md) | benchmarki |
| [docs/DECISIONS.md](docs/DECISIONS.md) | decyzje architektoniczne (ADR) |
| [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md) | współpraca, zgłaszanie podatności |

## Znane ograniczenia (skrót)

- JPEG XL jest zapisywany wyłącznie bezstratnie (brak stabilnego stratnego kodera bez CMake); AVIF — tylko stratnie.
- Animowane AVIF i JPEG XL są zachowywane jako oryginał (nie są ponownie kodowane); APNG nie jest zapisywany.
- Anulowanie działa między etapami — pojedyncze kodowanie dużego obrazu AVIF kończy się przed zatrzymaniem.
- Instalatory nie są podpisane cyfrowo (Windows SmartScreen / macOS Gatekeeper mogą ostrzegać).

Pełna lista: [docs/LIMITATIONS.md](docs/LIMITATIONS.md).

## Licencja

Imago jest wolnym oprogramowaniem na licencji **GNU GPL v3.0 lub nowszej** ([LICENSE](LICENSE)). Licencja wynika z użycia biblioteki libimagequant (stratna kompresja PNG). Licencje wszystkich zależności: [THIRD_PARTY.md](THIRD_PARTY.md).
