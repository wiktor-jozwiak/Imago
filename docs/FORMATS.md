# Obsługiwane formaty

Format jest zawsze rozpoznawany **po zawartości pliku** (sygnaturze), nie po rozszerzeniu. Plik `zdjecie.jpg`
zawierający PNG zostanie przetworzony jako PNG (z ostrzeżeniem). Wszystkie kodeki są wbudowane w aplikację.

| Format | Odczyt | Zapis | Tryby zapisu | Przezroczystość | Animacja | ICC | EXIF | Biblioteka |
|---|---|---|---|---|---|---|---|---|
| JPEG (`.jpg`, `.jpeg`, `.jpe`, `.jfif`) | ✓ (także CMYK) | ✓ | stratny | — (spłaszczenie tylko za zgodą) | — | ✓ | ✓ | zune-jpeg / MozJPEG |
| PNG (`.png`, `.apng`) | ✓ (8/16 bit, APNG) | ✓ | stratny (paleta) i bezstratny | ✓ | ✓ (APNG) | ✓ | ✓ | png / libimagequant + oxipng |
| WebP | ✓ (także animowany) | ✓ | stratny i bezstratny | ✓ | ✓ | ✓ | ✓ | image-webp / libwebp |
| AVIF | ✓ (także animowany) | ✓ | stratny | ✓ | ✓ | ✓ | ✓ | avif-decode (rav1d), zenavif (rav1d-safe, zenrav1e) |
| TIFF (`.tif`, `.tiff`) | ✓ (pierwsza strona) | ✓ | bezstratny (Deflate) | ✓ | — | ✓ | — | tiff |
| BMP | ✓ | — (zapisywany jako PNG) | — | ✓ | — | — | — | image |
| GIF | ✓ (także animowany) | ✓ | paleta 256 kolorów | 1-bitowa | ✓ | — | — | gif + libimagequant |
| JPEG XL (`.jxl`) | ✓ (także animowany) | ✓ *eksperymentalny* | stratny i bezstratny | ✓ | ✓ | ✓ | ✓ | jxl-oxide / jxl-encoder |
| HEIC/HEIF (`.heic`, `.heif`, `.hif`) | ✓ (zdjęcia z iPhone'a, siatki kafelków, 8/10 bit) | — (zapisywany jako JPEG) | — | ✓ | — | ✓ | ✓ | heic-rs |
| PDF, PSD, SVG, ICO, QOI… | rozpoznawane i czytelnie odrzucane | — | | | | | | |

JPEG XL jest oznaczony jako eksperymentalny: każdy wynik jest ponownie dekodowany przez jxl-oxide (tryb bezstratny —
piksel w piksel), a błąd weryfikacji kończy się czytelnym błędem zamiast złego pliku. Kodeki animowanego AVIF i JPEG XL
są na licencji AGPL-3.0 ([ADR-016](DECISIONS.md)).

## Jakość i tryby

- **Jakość 1–100** (domyślnie 82) steruje koderami stratnymi: JPEG (MozJPEG, przy ≥ 90 bez podpróbkowania chrominancji),
  WebP, AVIF, JPEG XL (skala skalibrowana do odległości butteraugli) oraz kwantyzacją PNG (libimagequant: minimum =
  jakość − 30).
- **Tryb bezstratny** — piksele są zachowane dokładnie (PNG, WebP bezstratny, TIFF, JPEG XL). JPEG i AVIF nie mają
  trybu bezstratnego; dla JPEG→JPEG Imago usuwa wtedy tylko metadane, bez ponownego kodowania.
- **Stratny PNG** działa jak TinyPNG: redukcja do ≤ 256 kolorów z ditheringiem. Grafiki, które już mają ≤ 256 kolorów,
  są zapisywane bez strat (paleta dokładna).
- **Maksymalna kompresja** — wolniejsze ustawienia: MozJPEG z trellis i optymalizacją skanów, WebP `method 6` + sharp YUV,
  AVIF `speed 3`, JPEG XL wysiłek 8, oxipng poziom 5, libimagequant `speed 1`.
- **Tryb szybki** — MozJPEG fastest, WebP `method 2`, AVIF `speed 9`, JPEG XL wysiłek 3, oxipng poziom 1; tryb auto
  pomija AVIF.

## Tryb automatyczny

| Rodzaj obrazu | Rozpoznanie | Kandydaci |
|---|---|---|
| Zdjęcie | > 256 kolorów, większe niż 256×256 | WebP, AVIF, JPEG |
| Z przezroczystością | kanał alfa faktycznie używany | WebP, PNG (alfa nigdy nie jest usuwana) |
| Prosta grafika | ≤ 256 kolorów | PNG (dokładna paleta), WebP bezstratny |
| Ikona | ≤ 256×256 px | PNG, WebP |
| Animacja | GIF/WebP/APNG/AVIF/JPEG XL z > 1 klatką | animowany WebP (przy maksymalnej kompresji także animowany AVIF) albo oryginał |
| Tryb bezstratny | — | PNG, WebP bezstratny |

Wygrywa najmniejszy wynik. Jeśli żaden nie jest mniejszy od oryginału, zapisywany jest oryginał (z metadanymi usuniętymi
bez ponownego kodowania, jeśli to możliwe). Kanał alfa, który jest w pełni nieprzezroczysty, jest usuwany bezstratnie.
JPEG XL nigdy nie jest wybierany automatycznie (słabe wsparcie przeglądarek). HEIC nie może zostać oryginałem — w trybie
„Zachowaj format” jest zapisywany jako JPEG.

## Animacje

- Animacje są **zawsze zachowywane**, chyba że wybrano „Tylko pierwsza klatka” (`--first-frame`).
- Wybrany format bez animacji (np. JPEG, TIFF) → animacja zostaje w formacie źródłowym, z ostrzeżeniem; w aplikacji
  pojawia się pytanie przed startem.
- **Animowany WebP** jest składany klatka po klatce bez trzymania całej animacji w pamięci: kodowany jest tylko zmieniony
  prostokąt, identyczne klatki są łączone. W trybie stratnym każda klatka jest próbowana jako „zastąp” i „nałóż”
  (niezmienione piksele przezroczyste) oraz stratnie i bezstratnie — zostaje najmniejszy wariant (jak WebPAnimEncoder).
- **APNG** — jedna wspólna paleta (dokładna przy ≤ 256 kolorach, w trybie stratnym z libimagequant), prostokąty zmian
  i nakładanie wybierane według rozmiaru, kompresja oxipng.
- **Animowany AVIF** — kodowanie międzyklatkowe AV1 (zenrav1e); **animowany JPEG XL** — jxl-encoder, stratnie lub
  bezstratnie. Oba wymagają wszystkich klatek naraz (limit `max_animation_pixels`).
- Animowane AVIF i JPEG XL są też odczytywane i mogą zostać zamienione na dowolny format animowany.

## Tryb dużych obrazów

Obraz przekraczający limit pikseli (domyślnie 150 MP) lub budżet pamięci jest — dla JPEG i PNG — przetwarzany
**pasami wierszy**: dekoder → konwersja do sRGB → zmniejszenie (uśrednianie pola) → spłaszczenie przezroczystości →
koder. Pamięć zależy od szerokości obrazu, nie od pola (panorama 30 000 × 20 000 px potrzebuje kilku MB zamiast ~2 GB).

- wejście: JPEG (bez CMYK), PNG bez przeplotu; wyjście: JPEG (baseline) lub PNG; tryb auto: JPEG, a przy
  przezroczystości PNG;
- maksymalnie 65 535 px na bok; bez powiększania i bez obrotu pikseli (orientacja EXIF zostaje jako znacznik);
- inne formaty ponad limit są odrzucane z komunikatem; tryb można wyłączyć w ustawieniach zaawansowanych
  lub opcją `--no-large-image-mode`.

## Limity wymiarów koderów

WebP: 16 383 px na bok · JPEG: 65 500 px · GIF: 65 535 px.
W trybie auto kandydat przekraczający limit jest pomijany; przy jawnie wybranym formacie pojawia się czytelny błąd.

## Format niedostępny w danym buildzie

Jeżeli aplikację zbudowano bez funkcji `avif`, `jxl` lub `heic`, format jest wyszarzony w interfejsie, CLI zwraca błąd
`format_unavailable` z proponowaną alternatywą (np. WebP) i instrukcją włączenia, a szczegóły trafiają do logu.
Lista w bieżącej wersji: `imago --list-formats`.
