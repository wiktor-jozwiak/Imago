# Obsługiwane formaty

Format jest zawsze rozpoznawany **po zawartości pliku** (sygnaturze), nie po rozszerzeniu. Plik `zdjecie.jpg`
zawierający PNG zostanie przetworzony jako PNG (z ostrzeżeniem). Wszystkie kodeki są wbudowane w aplikację.

| Format | Odczyt | Zapis | Tryby zapisu | Przezroczystość | Animacja | ICC | EXIF | Biblioteka |
|---|---|---|---|---|---|---|---|---|
| JPEG (`.jpg`, `.jpeg`, `.jpe`, `.jfif`) | ✓ (także CMYK) | ✓ | stratny | — (spłaszczenie tylko za zgodą) | — | ✓ | ✓ | zune-jpeg / MozJPEG |
| PNG (`.png`, `.apng`) | ✓ (8/16 bit, APNG) | ✓ | stratny (paleta) i bezstratny | ✓ | odczyt APNG | ✓ | ✓ | png / libimagequant + oxipng |
| WebP | ✓ (także animowany) | ✓ | stratny i bezstratny | ✓ | ✓ | ✓ | ✓ | image-webp / libwebp |
| AVIF | ✓ | ✓ | stratny | ✓ | wykrywanie (oryginał zachowany) | → sRGB | ✓ | rav1d / rav1e |
| TIFF (`.tif`, `.tiff`) | ✓ (pierwsza strona) | ✓ | bezstratny (Deflate) | ✓ | — | ✓ | — | tiff |
| BMP | ✓ | — (zapisywany jako PNG) | — | ✓ | — | — | — | image |
| GIF | ✓ (także animowany) | ✓ | paleta 256 kolorów | 1-bitowa | ✓ | — | — | gif + libimagequant |
| JPEG XL (`.jxl`) | ✓ | ✓ *eksperymentalny* | bezstratny | ✓ | wykrywanie (oryginał zachowany) | → sRGB | — | jxl-oxide / zune-jpegxl |
| HEIC/HEIF, PDF, PSD, SVG, ICO, QOI… | rozpoznawane i czytelnie odrzucane | — | | | | | | |

„→ sRGB” oznacza, że format nie przechowuje profilu ICC, więc obraz z szeroką gamą (Display P3, Adobe RGB)
jest najpierw konwertowany do sRGB, aby kolory pozostały poprawne.

## Jakość i tryby

- **Jakość 1–100** (domyślnie 82) steruje koderami stratnymi: JPEG (MozJPEG, przy ≥ 90 bez podpróbkowania chrominancji),
  WebP, AVIF oraz kwantyzacją PNG (libimagequant: minimum = jakość − 30).
- **Tryb bezstratny** — piksele są zachowane dokładnie (PNG, WebP bezstratny, TIFF, JPEG XL). JPEG i AVIF nie mają
  trybu bezstratnego; dla JPEG→JPEG Imago usuwa wtedy tylko metadane, bez ponownego kodowania.
- **Stratny PNG** działa jak TinyPNG: redukcja do ≤ 256 kolorów z ditheringiem. Grafiki, które już mają ≤ 256 kolorów,
  są zapisywane bez strat (paleta dokładna).
- **Maksymalna kompresja** — wolniejsze ustawienia: MozJPEG z trellis i optymalizacją skanów, WebP `method 6` + sharp YUV,
  AVIF `speed 3`, oxipng poziom 5, libimagequant `speed 1`.
- **Tryb szybki** — MozJPEG fastest, WebP `method 2`, AVIF `speed 9`, oxipng poziom 1; tryb auto pomija AVIF.

## Tryb automatyczny

| Rodzaj obrazu | Rozpoznanie | Kandydaci |
|---|---|---|
| Zdjęcie | > 256 kolorów, większe niż 256×256 | WebP, AVIF, JPEG |
| Z przezroczystością | kanał alfa faktycznie używany | WebP, PNG (alfa nigdy nie jest usuwana) |
| Prosta grafika | ≤ 256 kolorów | PNG (dokładna paleta), WebP bezstratny |
| Ikona | ≤ 256×256 px | PNG, WebP |
| Animacja | GIF/WebP/APNG z > 1 klatką | animowany WebP albo oryginał |
| Tryb bezstratny | — | PNG, WebP bezstratny |

Wygrywa najmniejszy wynik. Jeśli żaden nie jest mniejszy od oryginału, zapisywany jest oryginał (z metadanymi usuniętymi
bez ponownego kodowania, jeśli to możliwe). Kanał alfa, który jest w pełni nieprzezroczysty, jest usuwany bezstratnie.
JPEG XL nigdy nie jest wybierany automatycznie (słabe wsparcie przeglądarek).

## Animacje

- Animacje są **zawsze zachowywane**, chyba że wybrano „Tylko pierwsza klatka” (`--first-frame`).
- Wybrany format bez animacji (np. JPEG) → animacja zostaje w formacie źródłowym, z ostrzeżeniem; w aplikacji pojawia się
  pytanie przed startem.
- Animowany WebP jest składany klatka po klatce (kodowany jest tylko zmieniony prostokąt, identyczne klatki są łączone),
  bez trzymania całej animacji w pamięci.
- Animowane AVIF i JPEG XL oraz APNG nie są ponownie kodowane (oryginał jest kopiowany).

## Limity wymiarów koderów

WebP: 16 383 px na bok · JPEG: 65 500 px · GIF: 65 535 px · JPEG XL: min. 2×2 px.
W trybie auto kandydat przekraczający limit jest pomijany; przy jawnie wybranym formacie pojawia się czytelny błąd.

## Format niedostępny w danym buildzie

Jeżeli aplikację zbudowano bez funkcji `avif` lub `jxl`, format jest wyszarzony w interfejsie, CLI zwraca błąd
`format_unavailable` z proponowaną alternatywą (np. WebP) i instrukcją włączenia, a szczegóły trafiają do logu.
Lista w bieżącej wersji: `imago --list-formats`.
