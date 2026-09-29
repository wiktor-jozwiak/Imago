# Wydajność i benchmarki

## Jak to działa

- **Równoległość na poziomie plików** — pula `N` wątków (domyślnie rdzenie logiczne − 1) pobiera pliki z kolejki.
  Kodeki (rav1e, oxipng, libimagequant) dodatkowo korzystają z globalnej puli `rayon`.
- **Budżet pamięci** — przed dekodowaniem wątek rezerwuje szacowaną pamięć (piksele × bajty × współczynnik);
  duże obrazy czekają, aż zwolni się miejsce. Domyślnie 2 GB.
- **Brak zbędnej pracy** — nagłówek czytany jest przed dekodowaniem (limity, format), kandydaci większi od
  najlepszego są natychmiast zwalniani, identyczne klatki animacji są łączone, metadane można usunąć bez ponownego kodowania.
- **SIMD** — skalowanie `fast_image_resize` (AVX2/SSE4.1/NEON), MozJPEG i rav1e/rav1d z asemblerem (NASM).
- **Interfejs** — zdarzenia postępu są grupowane (~12 aktualizacji/s), lista plików jest wirtualizowana,
  miniatury renderowane leniwie po 3 naraz.

## Wyniki

Maszyna: AMD Ryzen 7 5700X (8 rdzeni / 16 wątków), 32 GB RAM, Windows 11, Rust 1.98.1, build `bench` (release).
Dane syntetyczne (gradienty + szum, „zdjęciopodobne”), jakość 82, wysiłek „zrównoważony”.
Wartości to mediany Criterion (skrócony czas pomiaru, orientacyjne).

### Kodowanie obrazu 1024 × 768 (0,8 MP)

| Format | Czas |
|---|---|
| JPEG (MozJPEG, progresywny, trellis) | 40 ms |
| WebP (libwebp, method 4) | 60 ms |
| AVIF (rav1e, speed 6) | 240 ms |
| TIFF (Deflate + predyktor) | 192 ms |
| PNG stratny (libimagequant + oxipng) | 388 ms |
| PNG bezstratny (oxipng poziom 2) | 383 ms |

### Wpływ jakości (800 × 600)

| Jakość | JPEG | WebP | AVIF |
|---|---|---|---|
| 50 | 24 ms | 25 ms | 109 ms |
| 75 | 29 ms | 31 ms | 223 ms |
| 90 | 67 ms | 44 ms | 177 ms |

(JPEG ≥ 90 koduje chrominancję w pełnej rozdzielczości 4:4:4, stąd skok czasu i jakości.)

### Dekodowanie (1024 × 768)

| Format | Czas | Przepustowość |
|---|---|---|
| JPEG | 2,6 ms | 308 Mpx/s |
| PNG | 4,8 ms | 163 Mpx/s |
| WebP bezstratny | 8,7 ms | 91 Mpx/s |
| AVIF | 5,5 ms | 144 Mpx/s |

### Skalowanie

| Operacja | Czas | Przepustowość |
|---|---|---|
| 1920×1080 → 960×540, Lanczos3 | 2,8 ms | 750 Mpx/s |
| 4000×3000 → 2000×1500, Lanczos3 | 16 ms | 747 Mpx/s |
| 4000×3000 → miniatura 256 px, biliniowo | 3,3 ms | 3,7 Gpx/s |
| 2048×2048 RGBA → 512×512 (alfa premultiplikowana) | 8,5 ms | 1,4 Gpx/s |

### Pełne konwersje (dekodowanie + kodowanie, 0,8 MP)

| Konwersja | Czas |
|---|---|
| JPEG → WebP | 53 ms |
| JPEG → AVIF | 245 ms |
| PNG → WebP | 65 ms |
| PNG → PNG (grafika z alfą 512×512) | 49 ms |
| WebP → AVIF | 268 ms |

### Pipeline (odczyt, analiza, dekodowanie, kodowanie, weryfikacja, zapis atomowy)

Jeden plik JPEG 1600 × 1200 (1 wątek):

| Tryb | Czas |
|---|---|
| → WebP | 131 ms |
| → JPEG | 121 ms |
| automatyczny (WebP + AVIF + JPEG, wybór najmniejszego) | 801 ms |

Paczka 32 zdjęć 800 × 600 → WebP, zależnie od liczby wątków:

| Wątki | Czas | Pliki/s | Przyspieszenie |
|---|---|---|---|
| 1 | 1,23 s | 26 | 1,0× |
| 2 | 0,75 s | 42 | 1,6× |
| 4 | 0,40 s | 80 | 3,1× |
| 8 | 0,22 s | 149 | 5,7× |
| 16 | 0,15 s | 210 | 8,1× |

Tryb automatyczny kosztuje kilka kodowań na plik (najdroższy jest AVIF). Gdy liczy się czas, użyj „Trybu szybkiego”
(`--fast`, bez AVIF) albo wybierz format ręcznie.

## Uruchamianie benchmarków

```bash
cargo bench -p imago-core                       # wszystko (kilkanaście minut)
cargo bench -p imago-core --bench codecs        # kodowanie, dekodowanie, jakość, skalowanie, konwersje
cargo bench -p imago-core --bench pipeline      # pojedynczy plik i paczka z 1–16 wątkami
cargo bench -p imago-core --bench codecs -- --warm-up-time 1 --measurement-time 3   # szybciej, mniej dokładnie
```

Raporty HTML Criterion trafiają do `target/criterion/`.
