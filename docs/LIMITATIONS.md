# Znane ograniczenia

## Formaty

- **JPEG XL** — koder `jxl-encoder` jest młody i oznaczony jako eksperymentalny: każdy wynik jest weryfikowany pełnym
  dekodowaniem (bezstratny — piksel w piksel). JPEG XL nigdy nie jest wybierany w trybie automatycznym.
- **AVIF** — brak trybu bezstratnego (tryb „kwantyzator 0” kodera nie jest dokładny co do bitu); kadrowanie `clap`
  nie jest uwzględniane przy odczycie (orientacja `irot`/`imir` — tak).
- **Animowane AVIF, JPEG XL i APNG** — do zapisu wszystkie klatki są trzymane w pamięci (kodowanie międzyklatkowe,
  wspólna paleta), co ogranicza `max_animation_pixels`. APNG jest zawsze 8-bitowy (RGBA albo paleta).
- **HEIC/HEIF** — tylko odczyt (koder HEVC nie istnieje w czystym Ruście); obsługiwane zdjęcia HEVC z kafelkami,
  8/10 bit. Sekwencje HEIF (Live Photo, `msf1`), obrazy nakładane (`iovl`) i warstwowe są odrzucane z komunikatem.
  HDR (PQ/HLG) nie jest mapowany tonalnie.
- **GIF** — zapis tylko przy zachowaniu animacji lub gdy GIF wybrano jawnie; paleta 256 kolorów i 1-bitowa przezroczystość.
  Ponowne kodowanie GIF rzadko zmniejsza plik — tryb auto zwykle wybiera animowany WebP albo zostawia oryginał.
- **TIFF** — odczytywana jest tylko pierwsza strona; zapis zawsze bezstratny, EXIF nie jest przenoszony.
- **CMYK JPEG** — dekoder konwertuje CMYK do RGB bez użycia profilu ICC CMYK (kolory mogą się nieznacznie różnić);
  profil CMYK jest wtedy pomijany. W trybie dużych obrazów CMYK nie jest obsługiwany.
- **RAW, PSD, SVG, PDF** — nieobsługiwane (rozpoznawane i czytelnie odrzucane).
- **16 bitów na kanał** — zachowywane w PNG, TIFF i JPEG XL bezstratnym; AVIF zapisuje 10 bitów; pozostałe formaty
  zapisują 8 bitów.

## Przetwarzanie

- Kodowanie AVIF i JPEG XL da się przerwać w trakcie obrazu; pozostałe kodery (MozJPEG, libwebp, oxipng) kończą
  bieżący obraz przed zatrzymaniem.
- Tryb automatyczny koduje kilku kandydatów, więc jest wolniejszy niż jawnie wybrany format (zob. [PERFORMANCE.md](PERFORMANCE.md)).
- Budżet pamięci jest szacunkiem; bardzo nietypowe pliki (np. ogromne animacje) mogą chwilowo użyć więcej —
  dlatego istnieją twarde limity rozmiaru, wymiarów i łącznej liczby pikseli animacji.
- Animowany WebP w trybie bezstratnym nie używa nakładania klatek (dekodery liczą je w przybliżeniu), więc bywa
  większy niż w trybie stratnym z mieszanymi klatkami (ADR-017).
- **Tryb dużych obrazów** obsługuje tylko JPEG (bez CMYK) i PNG bez przeplotu, do 65 535 px na bok; zapisuje JPEG
  baseline ze standardowymi tablicami Huffmana (~5–10% większy niż zoptymalizowany), nie obraca pikseli i nie
  powiększa (ADR-018).

## Aplikacja

- Instalatory są podpisywane tylko wtedy, gdy właściciel skonfiguruje certyfikat w CI ([SIGNING.md](SIGNING.md));
  bez niego Windows SmartScreen i macOS Gatekeeper mogą wyświetlić ostrzeżenie. Integralność plików można sprawdzić
  z `SHA256SUMS.txt` dołączanym do każdego wydania.
- Brak automatycznych aktualizacji (celowo — aplikacja nie łączy się z internetem).
- Instalator Windows może pobrać WebView2, jeśli nie jest zainstalowany (Windows 10 bez aktualizacji).
- Ścieżki, które nie są poprawnym UTF-8 (możliwe tylko na Linuksie), działają w CLI, ale nie mogą zostać przekazane
  do interfejsu graficznego.
- Podgląd i miniatury są generowane przez silnik (także dla TIFF/AVIF/JXL), więc dla bardzo dużych plików pojawiają się
  z opóźnieniem.
- CLI jest dystrybuowane jako osobne archiwum; plik wykonywalny aplikacji nazywa się `imago-app` (skrót i nazwa w systemie: „Imago”), aby nie kolidował z `imago` na systemach plików bez rozróżniania wielkości liter.
