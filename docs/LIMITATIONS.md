# Znane ograniczenia

## Formaty

- **JPEG XL** — zapis wyłącznie bezstratny (koder `zune-jpegxl`, oznaczony jako eksperymentalny; każdy wynik jest
  weryfikowany pełnym dekodowaniem). Stratny JPEG XL wymagałby libjxl budowanego przez CMake. Obrazy mniejsze niż 2×2 px
  nie mogą być zapisane jako JPEG XL. JPEG XL nigdy nie jest wybierany w trybie automatycznym.
- **AVIF** — brak trybu bezstratnego (koder rav1e); profile ICC nie są osadzane (kolory są konwertowane do sRGB);
  orientacja zapisana w kontenerze AVIF (`irot`/`imir`) i kadrowanie (`clap`) nie są uwzględniane przy odczycie.
- **Animowane AVIF, animowane JPEG XL i APNG** — są wykrywane, ale nie są ponownie kodowane (zachowywany jest oryginał).
  Opcja „Tylko pierwsza klatka” pozwala je przekonwertować do obrazu statycznego.
- **GIF** — zapis tylko przy zachowaniu animacji lub gdy GIF wybrano jawnie; paleta 256 kolorów i 1-bitowa przezroczystość.
  Ponowne kodowanie GIF rzadko zmniejsza plik — tryb auto zwykle wybiera animowany WebP albo zostawia oryginał.
- **TIFF** — odczytywana jest tylko pierwsza strona; zapis zawsze bezstratny, EXIF nie jest przenoszony.
- **CMYK JPEG** — dekoder konwertuje CMYK do RGB bez użycia profilu ICC CMYK (kolory mogą się nieznacznie różnić);
  profil CMYK jest wtedy pomijany.
- **HEIC/HEIF, RAW, PSD, SVG, PDF** — nieobsługiwane (rozpoznawane i czytelnie odrzucane).
- **16 bitów na kanał** — zachowywane w PNG, TIFF i JPEG XL bezstratnym; pozostałe formaty zapisują 8 bitów.

## Przetwarzanie

- Anulowanie działa między etapami — trwające kodowanie jednego obrazu (np. dużego AVIF) kończy się przed zatrzymaniem.
- Tryb automatyczny koduje kilku kandydatów, więc jest wolniejszy niż jawnie wybrany format (zob. [PERFORMANCE.md](PERFORMANCE.md)).
- Budżet pamięci jest szacunkiem; bardzo nietypowe pliki (np. ogromne animacje) mogą chwilowo użyć więcej —
  dlatego istnieją twarde limity rozmiaru, wymiarów i łącznej liczby pikseli animacji.
- Animowany WebP jest składany z niezależnie kodowanych klatek (zmienione prostokąty) — kompresja bywa nieco słabsza
  niż w `WebPAnimEncoder` (ADR-012).

## Aplikacja

- Instalatory nie są podpisane cyfrowo: Windows SmartScreen i macOS Gatekeeper mogą wyświetlić ostrzeżenie.
- Brak automatycznych aktualizacji (celowo — aplikacja nie łączy się z internetem).
- Instalator Windows może pobrać WebView2, jeśli nie jest zainstalowany (Windows 10 bez aktualizacji).
- Ścieżki, które nie są poprawnym UTF-8 (możliwe tylko na Linuksie), działają w CLI, ale nie mogą zostać przekazane
  do interfejsu graficznego.
- Podgląd i miniatury są generowane przez silnik (także dla TIFF/AVIF/JXL), więc dla bardzo dużych plików pojawiają się
  z opóźnieniem.
- CLI jest dystrybuowane jako osobne archiwum; plik wykonywalny aplikacji nazywa się `imago-app` (skrót i nazwa w systemie: „Imago”), aby nie kolidował z `imago` na systemach plików bez rozróżniania wielkości liter.
