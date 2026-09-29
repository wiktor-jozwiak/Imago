# Pipeline przetwarzania pliku

Implementacja: [`crates/imago-core/src/image_engine/pipeline.rs`](../crates/imago-core/src/image_engine/pipeline.rs).
Każdy plik przechodzi 23 kroki ze specyfikacji. Każdy krok zwraca kontrolowany błąd (`ImagoError` ze stabilnym kodem),
sprawdza token anulowania, jest mierzony (`StageTimings`) i logowany (`tracing`, span z identyfikatorem i ścieżką).
Cały plik jest przetwarzany wewnątrz `catch_unwind` — ewentualna panika biblioteki kończy się statusem *Błąd*
tylko dla tego pliku.

| # | Krok | Etap UI | Jak | Błędy |
|---|---|---|---|---|
| 1 | Walidacja ścieżki | Odczyt | `metadata()`: istnieje, zwykły plik | `not_found`, `not_a_file` |
| 2 | Uprawnienia | Odczyt | odczyt pliku | `permission_denied` |
| 3 | Rozmiar pliku | Odczyt | limit `maxFileBytes` (domyślnie 512 MB) | `file_too_large` |
| 4 | Sygnatura | Analiza | pierwsze 64 bajty | `unsupported_format` |
| 5 | Prawdziwy format | Analiza | `detection::sniff`, ostrzeżenie przy niezgodnym rozszerzeniu | `unsupported_format` |
| 6 | Wymiary z nagłówka | Analiza | dekodery bez pikseli / `avif-parse` / `jxl-oxide`; limity wymiarów i pikseli | `dimensions_too_large`, `decode_failed` |
| 7 | Animacja | Analiza | GIF: liczba klatek; WebP: flaga ANIM; PNG: `acTL`; AVIF: marka `avis` | |
| 8 | Kanał alfa | Analiza | typ koloru z nagłówka; po dekodowaniu faktyczne użycie alfy | |
| 9 | Metadane | Analiza | EXIF (orientacja), ICC (także z kontenera) | |
| 10 | Plan | Analiza | auto / oryginał / format; animacje; przezroczystość; tryb bezstratny; wcześniejsze „pomiń istniejące” | `invalid_settings`, `format_unavailable` |
| — | Budżet pamięci | — | rezerwacja szacowanej pamięci (czeka, jeśli trzeba) | `cancelled` |
| 11 | Dekodowanie | Dekodowanie | z `image::Limits`, osłona przed paniką | `decode_failed` |
| 12 | Orientacja EXIF | Przekształcanie | `apply_orientation`; EXIF zachowany → orientacja = 1 | |
| 13 | Zmiana wymiarów | Przekształcanie | `fast_image_resize` Lanczos3, alfa premultiplikowana, bez powiększania | |
| 14 | Przestrzeń kolorów | Przekształcanie | alfa w pełni nieprzezroczysta → usunięta; ICC ≠ sRGB → zachowany albo konwersja do sRGB | |
| 15 | Metadane wyjściowe | Przekształcanie | EXIF usuwany domyślnie; orientacja zachowana, gdy pikseli nie obrócono | |
| 16 | Kodowanie | Kodowanie | każdy kandydat; większe wyniki od razu zwalniane; w auto nieudany kandydat jest pomijany | `encode_failed` |
| 17 | Porównanie z oryginałem | Kodowanie | oryginał (ew. bez metadanych, bez rekodowania) jest kandydatem, gdy dozwolone | |
| 18 | Wybór wariantu | Kodowanie | najmniejszy; uzasadnienie decyzji w raporcie | |
| 19 | Plik tymczasowy | Zapis | `.imago-*.tmp` w katalogu docelowym (ten sam system plików) | `permission_denied`, `disk_full` |
| 20 | Weryfikacja | Zapis | odczyt z dysku bajt w bajt + rozpoznanie sygnatury (JXL: dodatkowo pełny round-trip w koderze) | `verification_failed` |
| 21 | Atomowe przeniesienie | Zapis | `persist_noclobber` (bez nadpisania) lub `persist` (za zgodą); konflikt z innym programem → nowa nazwa | `output_exists` |
| 22 | Postęp | — | zdarzenia `fileStarted` / `fileProgress` / `fileFinished` | |
| 23 | Wynik | — | `FileReport`: rozmiary, wymiary, redukcja, czasy, decyzja, ostrzeżenia | |

## Bezpieczny zapis

1. `tempfile` tworzy plik z losową nazwą (`O_EXCL`, uprawnienia 0600) **w katalogu docelowym**.
2. Zapis danych, `flush`.
3. `fsync` (domyślnie włączony — ustawienie „Wymuś zapis na dysk”).
4. Weryfikacja pliku tymczasowego (krok 20). Niepowodzenie → plik tymczasowy usunięty, cel nietknięty.
5. Atomowe przeniesienie: `persist_noclobber` nie nadpisze pliku, który pojawił się w międzyczasie; `persist`
   (tylko przy polityce „Nadpisz”) zastępuje cel atomowo (Windows `MoveFileExW`, Unix `rename`).
6. Plik tymczasowy znika zawsze (przeniesiony albo usunięty przez `Drop` — także przy anulowaniu i panice).
   Po twardym przerwaniu procesu pozostałe `.imago-*.tmp` starsze niż godzina są usuwane przy następnym zadaniu w tym folderze.

Oryginał nigdy nie jest otwierany do zapisu. Nawet przy świadomym nadpisywaniu oryginałów wynik powstaje najpierw
w pliku tymczasowym, więc awaria w trakcie nie uszkodzi pliku źródłowego.

## Konflikty nazw

- **Dodaj numer** (domyślnie): `zdjecie.webp` → `zdjecie (1).webp`.
- **Pomiń**: istniejący wynik zostaje, plik dostaje status *Pominięto*.
- **Nadpisz**: wymaga potwierdzenia w aplikacji. Nadpisanie oryginału dodatkowo wymaga opcji
  „Pozwól nadpisywać pliki oryginalne”, inaczej błąd `would_overwrite_original`.
- Dwa wejścia prowadzące do tej samej nazwy wyjściowej (np. `a.png` i `a.jpg` → `a.webp`) nigdy się nie nadpisują —
  rejestr nazw zadania nadaje drugiemu numer.

## Anulowanie

Token zadania i tokeny plików (hierarchia): „Anuluj” zatrzymuje wszystko, „Pomiń” — jeden plik. Tokeny są sprawdzane
między krokami, między kandydatami kodowania i między klatkami animacji. Pojedyncze wywołanie kodera (np. kodowanie
jednego dużego AVIF) nie jest przerywane w połowie.
