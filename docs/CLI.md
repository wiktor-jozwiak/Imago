# Imago — wiersz poleceń (`imago`)

`imago` używa dokładnie tego samego silnika i tych samych ustawień co aplikacja desktopowa.
Wszystko dzieje się lokalnie — narzędzie nie łączy się z internetem.

```
imago [OPCJE] <WEJŚCIE>...
```

`WEJŚCIE` to pliki i/lub foldery. Foldery są przeszukiwane **bez podfolderów**, chyba że podasz `--recursive`.
Pliki są rozpoznawane po zawartości, nie po rozszerzeniu; nieobsługiwane pliki w folderach są pomijane.

## Przykłady

```bash
# jeden plik → WebP o podanej nazwie
imago input.jpg --output optimized.webp --quality 82

# jeśli rozszerzenie nie pasuje do --format, zostanie poprawione (optimized.webp) z ostrzeżeniem
imago input.jpg --output optimized.jpg --format webp --quality 82

# cały folder, automatyczny wybór formatu (domyślny)
imago ./photos --output ./optimized --format auto --quality 82

# z podfolderami, 8 wątków
imago ./photos --output ./optimized --recursive --workers 8

# szerokość 2400 px (wysokość proporcjonalnie), bez metadanych
imago ./photos --output ./optimized --width 2400 --remove-metadata

# AVIF o jakości 70
imago ./photos --output ./optimized --format avif --quality 70

# bezstratnie (PNG/WebP według trybu auto)
imago ./photos --output ./optimized --lossless

# pomiń pliki, które już istnieją w folderze wyjściowym
imago ./photos --output ./optimized --skip-existing

# wyniki spakuj do ZIP i wypisz raport JSON
imago ./photos --zip photos.zip --json > raport.json

# tylko zmniejsz zdjęcia większe niż 1920×1080, zachowaj format
imago ./photos --format original --max-width 1920 --max-height 1080
```

Bez `--output` wyniki trafiają do folderu `optimized` obok wejść (jak w aplikacji). Istniejące pliki wynikowe
nie są nadpisywane — nowy wynik dostaje nazwę `zdjecie (1).webp`, chyba że użyjesz `--skip-existing` lub `--overwrite`.

## Opcje

### Wyjście

| Opcja | Opis |
|---|---|
| `-o, --output <ŚCIEŻKA>` | Folder wyjściowy; przy jednym pliku wejściowym może to być nazwa pliku wynikowego (z rozszerzeniem obrazu). |
| `--suffix <TEKST>` | Przyrostek dodawany do nazw, np. `-min` → `zdjecie-min.webp`. |
| `--flat` | Wszystkie wyniki w jednym folderze (bez odtwarzania podfolderów). |
| `--skip-existing` | Pomija wejścia, których wynik już istnieje. |
| `--overwrite` | Zastępuje istniejące pliki wynikowe. |
| `--allow-overwrite-originals` | Pozwala zastąpić same pliki wejściowe (tylko z `--overwrite`). Zapis i tak idzie przez plik tymczasowy. |
| `--force` | Zapisuje wynik nawet, gdy jest większy od oryginału (domyślnie zostaje oryginał). |
| `--zip <PLIK>` | Pakuje wszystkie wyniki do archiwum ZIP (bez ponownej kompresji, ZIP64 dla dużych plików). |
| `--no-fsync` | Nie wymusza zapisu na dysk przed przeniesieniem pliku (szybciej, mniej bezpiecznie). |

### Format i jakość

| Opcja | Opis |
|---|---|
| `-f, --format <FORMAT>` | `auto` (domyślnie), `original`, `jpeg`, `png`, `webp`, `avif`, `tiff`, `gif`, `jxl`. |
| `--quality <1-100>` | Jakość, domyślnie 82. |
| `--lossless` | Kompresja bezstratna (PNG, WebP, TIFF, JPEG XL). Dla JPEG→JPEG oznacza usunięcie metadanych bez ponownego kodowania. |
| `--max-compression` | Maksymalny wysiłek koderów (wolniej, mniejsze pliki). |
| `--fast` | Najszybsze ustawienia koderów (tryb auto pomija AVIF). |
| `--first-frame` | Animacje zapisuje jako pierwszą klatkę (bez tej opcji animacja jest zawsze zachowywana). |
| `--flatten` | Pozwala zastąpić przezroczystość białym tłem (np. przy `--format jpeg`). |

### Skalowanie

| Opcja | Opis |
|---|---|
| `--width <PX>`, `--height <PX>` | Docelowa szerokość/wysokość; druga wartość proporcjonalnie. Obie naraz: obraz mieści się w prostokącie. |
| `--stretch` | Przy `--width` i `--height`: dokładnie te wymiary, bez zachowania proporcji. |
| `--max-width <PX>`, `--max-height <PX>` | Zmniejsza tylko obrazy większe od limitu. |
| `--scale <PROCENT>` | Skala procentowa, np. `50`. |
| `--allow-upscale` | Pozwala powiększać obrazy (domyślnie nigdy). |

### Metadane

| Opcja | Opis |
|---|---|
| `--remove-metadata` | Usuwa EXIF (GPS, aparat), XMP i komentarze — **domyślnie włączone**. |
| `--keep-metadata` | Zachowuje EXIF (orientacja jest korygowana, jeśli piksele zostały obrócone). |
| `--strip-icc` | Konwertuje obrazy z szeroką gamą kolorów do sRGB i usuwa profil ICC. |
| `--preserve-timestamps` | Kopiuje datę modyfikacji wejścia do wyniku. |
| `--no-auto-orient` | Nie obraca pikseli wg EXIF (orientacja zostaje zapisana w metadanych). |

### Wejścia, wydajność, limity

| Opcja | Opis |
|---|---|
| `-r, --recursive` | Uwzględnia podfoldery. |
| `--include-hidden` | Uwzględnia ukryte pliki i foldery. |
| `-j, --workers <N>` | Liczba wątków (domyślnie liczba rdzeni logicznych − 1). |
| `--max-file-size <MB>` | Największy akceptowany plik (domyślnie 512 MB). |
| `--max-megapixels <MP>` | Największy akceptowany obraz (domyślnie 150 MP). |
| `--memory-budget <MB>` | Pamięć na jednocześnie dekodowane obrazy (domyślnie 2048 MB). |
| `--no-large-image-mode` | Odrzuca obrazy ponad limity zamiast przetwarzać JPEG i PNG pasami (tryb dużych obrazów, [FORMATS.md](FORMATS.md)). |

### Raportowanie

| Opcja | Opis |
|---|---|
| `-q, --quiet` | Tylko błędy. |
| `-v` / `-vv` | Decyzje trybu auto, ostrzeżenia i czasy etapów / dodatkowo log diagnostyczny. |
| `--json` | Raport JSON na standardowe wyjście (komunikaty ludzkie idą na stderr). |
| `--list-formats` | Tabela obsługiwanych formatów w tej wersji. |

Zmienna środowiskowa `IMAGO_LOG` (np. `IMAGO_LOG=debug`) steruje logiem diagnostycznym niezależnie od `-v`.

## Kody wyjścia

| Kod | Znaczenie |
|---|---|
| `0` | wszystkie pliki zoptymalizowane lub celowo pominięte |
| `1` | część plików zakończyła się błędem (pozostałe zostały zapisane) |
| `2` | nieprawidłowe argumenty |
| `3` | nic do zrobienia lub błąd krytyczny (brak obrazów, brak wejścia, nieprawidłowe ustawienia, błąd ZIP) |
| `130` | przerwane przez Ctrl+C |

Pierwsze Ctrl+C anuluje zadanie łagodnie (pliki tymczasowe są usuwane, zapisane wyniki zostają), drugie kończy proces natychmiast.

## Format raportu JSON

```json
{
  "summary": { "total": 2, "done": 2, "skipped": 0, "failed": 0, "cancelled": 0,
               "inputBytes": 1053000, "outputBytes": 55000, "elapsedMs": 212.4, "wasCancelled": false },
  "files": [
    { "id": 0, "inputPath": "photos/a.png", "outputPath": "photos/optimized/a.webp", "status": "done",
      "inputFormat": "png", "outputFormat": "webp", "inputSize": 793377, "outputSize": 28414,
      "inputDimensions": [640, 480], "outputDimensions": [640, 480], "reductionPercent": 96.4,
      "timings": { "readMs": 1.2, "decodeMs": 2.0, "resizeMs": 0.0, "encodeMs": 198.1, "writeMs": 7.3, "totalMs": 209.0 },
      "decision": { "format": "webp", "reason": "smallest", "savedPercent": 96.4, "keptTransparency": false,
                    "keptAnimation": false, "candidates": [ { "format": "webp", "lossless": false, "size": 28414 } ] },
      "warnings": [], "error": null }
  ],
  "archive": null
}
```

`status`: `done`, `skipped`, `failed`, `cancelled`. Przy błędzie `error` zawiera stabilny `code`
(np. `decode_failed`, `disk_full`, `permission_denied`) i opis `message`.
