# Imago — specyfikacja projektu (oryginalne wymagania)

> Ten plik zawiera pierwotną specyfikację projektu, na podstawie której budowana jest aplikacja.
> Jest źródłem prawdy dla wymagań. Status realizacji poszczególnych punktów śledzi
> [REQUIREMENTS.md](REQUIREMENTS.md), a postęp etapów [PROGRESS.md](PROGRESS.md).
> Świadome odstępstwa od specyfikacji są opisane w [DECISIONS.md](DECISIONS.md).

---

Jesteś seniorem Rust developerem, architektem oprogramowania, inżynierem systemowym oraz specjalistą od wydajnego przetwarzania obrazów.

Twoim zadaniem jest zaprojektowanie i zbudowanie profesjonalnej aplikacji desktopowej o nazwie Imago.

## NAZWA APLIKACJI

Nazwa: Imago

Pełna nazwa: Imago Image Optimizer

Nazwa CLI: imago

Nazwa głównego crate'a: imago-core

Hasło aplikacji:

Fast. Local. Smaller images.

Polska wersja hasła:

Szybsze obrazy. Mniejsze pliki. Zero chmury.

Nie używaj nazw ImageForge, OptiPix, TinyForge ani żadnych wcześniejszych nazw roboczych.

## CEL PROJEKTU

Zbuduj darmową, natywną i bardzo wydajną aplikację desktopową do:

- kompresji obrazów;
- konwersji formatów;
- zmiany rozmiaru;
- optymalizacji metadanych;
- przetwarzania wielu plików;
- przetwarzania całych folderów;
- automatycznego wyboru najlepszego formatu;
- tworzenia paczek ZIP;
- pracy z poziomu interfejsu graficznego;
- pracy z poziomu terminala.

Aplikacja ma być odpowiednikiem narzędzi internetowych takich jak Tinify, ale działać lokalnie na komputerze użytkownika.

Najważniejsze wymagania:

- brak logowania;
- brak reklam;
- brak płatnego API;
- brak telemetrii;
- brak zewnętrznych trackerów;
- brak wysyłania plików do internetu;
- działanie offline;
- przetwarzanie plików wyłącznie lokalnie;
- niskie zużycie pamięci;
- maksymalna możliwa wydajność;
- stabilność przy dużej liczbie plików;
- obsługa Windows, macOS i Linux.

Priorytety projektu, w kolejności:

1. poprawność wyniku;
2. bezpieczeństwo plików użytkownika;
3. lokalne przetwarzanie;
4. stabilność;
5. niskie zużycie pamięci;
6. szybkość;
7. wygodny interfejs;
8. łatwość dalszego rozwoju.

## TECHNOLOGIA

Backend:

- Rust stable;
- Tauri 2;
- serde;
- thiserror;
- anyhow;
- tracing;
- tracing-subscriber;
- rayon albo własna pula workerów;
- criterion do benchmarków;
- cargo test;
- cargo fmt;
- cargo clippy.

Frontend:

- React;
- TypeScript;
- Vite;
- nowoczesny CSS albo Tailwind CSS;
- komunikacja z backendem przez komendy Tauri.

Przetwarzanie obrazów:

- preferuj libvips przez stabilne bindingi Rust, jeśli zapewni najlepszą wydajność i stabilność;
- używaj crate'a image jako fallbacku lub do operacji, których libvips nie obsługuje wystarczająco dobrze;
- nie implementuj samodzielnie kodeków obrazów;
- nie używaj płatnych usług;
- nie buduj własnego serwera;
- nie wysyłaj żadnych obrazów do zewnętrznych usług.

Dodatkowe biblioteki mogą obejmować:

- zip do tworzenia archiwów;
- serde_json albo toml do konfiguracji;
- native-dialog lub mechanizmy Tauri do wyboru plików;
- num_cpus albo odpowiednik do wykrywania liczby rdzeni;
- uuid, jeśli potrzebne do bezpiecznych plików tymczasowych;
- walkdir do rekurencyjnego skanowania folderów.

Jeśli libvips wymaga zależności systemowych:

- przygotuj jasną instrukcję instalacji;
- sprawdzaj dostępność wymaganych bibliotek;
- pokaż czytelny komunikat błędu;
- przygotuj fallback, jeśli jest to możliwe;
- opisz różnice między buildem developerskim i release;
- przygotuj instrukcje dla Windows, macOS i Linux.

## OBSŁUGIWANE FORMATY WEJŚCIOWE

Obsłuż:

- JPG;
- JPEG;
- PNG;
- WebP;
- AVIF;
- TIFF;
- BMP;
- GIF;
- JPEG XL, jeśli dostępna jest stabilna biblioteka.

Dla GIF oraz animowanych WebP:

- wykrywaj animację;
- zachowuj animację domyślnie;
- nie konwertuj animacji do pojedynczej klatki bez wyraźnej zgody użytkownika;
- wyświetl ostrzeżenie, jeśli wybrany format wyjściowy nie obsługuje animacji;
- pozwól użytkownikowi zdecydować, czy animacja ma zostać zachowana.

## OBSŁUGIWANE FORMATY WYJŚCIOWE

Obsłuż:

- JPEG;
- PNG;
- WebP;
- AVIF;
- TIFF;
- JPEG XL, jeśli dostępny.

Jeśli dany format nie jest dostępny na konkretnej platformie:

- aplikacja nie może się zamknąć;
- pokaż czytelny komunikat;
- zaproponuj alternatywny format;
- zapisz szczegóły techniczne w logu;
- poinformuj użytkownika, jak włączyć obsługę danego formatu.

## FUNKCJE APLIKACJI

Zaimplementuj następujące funkcje:

1. Przeciąganie plików do okna.
2. Przeciąganie folderów do okna.
3. Wybór plików przez natywne okno systemowe.
4. Wybór folderu przez natywne okno systemowe.
5. Obsługa pojedynczego pliku.
6. Obsługa wielu plików.
7. Obsługa całych folderów.
8. Obsługa podfolderów.
9. Opcja rekurencyjnego skanowania folderów.
10. Filtrowanie nieobsługiwanych plików.
11. Pomijanie ukrytych plików, jeśli użytkownik to wybierze.
12. Pasek postępu globalnego.
13. Pasek postępu dla każdego pliku.
14. Możliwość anulowania całego zadania.
15. Możliwość pominięcia pojedynczego pliku.
16. Możliwość ponowienia nieudanego zadania.
17. Wybór formatu wyjściowego.
18. Tryb zachowania oryginalnego formatu.
19. Tryb automatycznego wyboru formatu.
20. Ustawienie jakości od 1 do 100.
21. Tryb bezstratny.
22. Tryb maksymalnej kompresji.
23. Zmiana szerokości obrazu.
24. Zmiana wysokości obrazu.
25. Zachowywanie proporcji.
26. Ustawienie maksymalnej szerokości.
27. Ustawienie maksymalnej wysokości.
28. Skalowanie procentowe.
29. Obrót zgodny z orientacją EXIF.
30. Usuwanie metadanych EXIF.
31. Opcjonalne zachowanie profilu ICC.
32. Opcjonalne zachowanie daty modyfikacji pliku.
33. Wybór folderu wyjściowego.
34. Automatyczne tworzenie folderu optimized.
35. Pomijanie istniejących plików.
36. Nadpisywanie istniejących plików po potwierdzeniu.
37. Dodawanie przyrostka do nazw plików.
38. Zachowywanie struktury folderów.
39. Pobieranie pojedynczych wyników.
40. Tworzenie paczki ZIP.
41. Podgląd obrazu przed i po kompresji.
42. Wyświetlanie wymiarów obrazu.
43. Wyświetlanie rozmiaru przed i po kompresji.
44. Wyświetlanie procentu redukcji.
45. Wyświetlanie czasu przetwarzania.
46. Wyświetlanie liczby przetworzonych plików.
47. Wyświetlanie liczby pominiętych plików.
48. Wyświetlanie liczby błędów.
49. Tryb jasny.
50. Tryb ciemny.
51. Zapisywanie ustawień użytkownika.
52. Obsługa skrótów klawiaturowych.
53. Sortowanie listy plików.
54. Filtrowanie plików po statusie.
55. Otwieranie folderu wynikowego w systemowym menedżerze plików.

## TRYB AUTOMATYCZNY

Zaprojektuj inteligentny tryb automatyczny.

Tryb automatyczny powinien:

- analizować typ obrazu;
- wykrywać kanał alfa;
- wykrywać animację;
- uwzględniać wymiary obrazu;
- uwzględniać rozmiar pliku;
- wybierać najlepszy format;
- porównywać rozmiar wyniku z oryginałem;
- nie zapisywać wyniku, jeśli jest większy od oryginału, chyba że użytkownik wymusi konwersję;
- zachowywać przezroczystość;
- zachowywać proporcje;
- nie skalować obrazu, jeśli nie przekracza limitów;
- unikać ponownego kodowania, jeśli nie przynosi korzyści.

Zasady wyboru formatu:

- dla zdjęć rozważ JPEG, WebP albo AVIF;
- dla obrazów z przezroczystością rozważ WebP albo PNG;
- dla prostych grafik rozważ PNG albo WebP;
- dla ikon rozważ PNG albo WebP;
- dla animacji zachowaj animowany GIF lub WebP, jeśli to możliwe;
- dla bardzo dużych zdjęć rozważ AVIF lub WebP;
- jeśli wynik jest większy niż oryginał, wybierz mniejszy wariant;
- nie usuwaj kanału alfa bez zgody użytkownika.

Domyślne ustawienia:

- jakość: 82;
- liczba workerów: liczba rdzeni logicznych minus jeden;
- minimalnie jeden worker;
- zachowanie proporcji: włączone;
- usuwanie metadanych: włączone;
- automatyczny wybór formatu: włączony;
- tworzenie podfolderu optimized: włączone;
- zachowanie oryginałów: włączone.

Tryb automatyczny powinien informować użytkownika o decyzji.

Przykładowy komunikat:

Wybrano WebP, ponieważ zmniejszył rozmiar pliku o 71% i zachował przezroczystość.

## ARCHITEKTURA

Podziel projekt na warstwy:

1. Warstwa interfejsu.
2. Warstwa komend Tauri.
3. Warstwa aplikacyjna.
4. Warstwa kolejki zadań.
5. Warstwa workerów.
6. Warstwa przetwarzania obrazów.
7. Warstwa kodeków.
8. Warstwa metadanych.
9. Warstwa systemu plików.
10. Warstwa archiwizacji.
11. Warstwa konfiguracji.
12. Warstwa logowania.
13. Warstwa CLI.

Sugerowane moduły backendu:

- app;
- commands;
- image_engine;
- codecs;
- pipeline;
- jobs;
- workers;
- metadata;
- filesystem;
- archive;
- config;
- logging;
- cli;
- errors;
- benchmarks.

## PIPELINE PRZETWARZANIA

Pipeline dla każdego pliku powinien wyglądać następująco:

1. Walidacja ścieżki.
2. Sprawdzenie uprawnień.
3. Sprawdzenie rozmiaru pliku.
4. Sprawdzenie sygnatury pliku.
5. Wykrycie prawdziwego formatu.
6. Odczyt wymiarów bez pełnego dekodowania, jeśli jest to możliwe.
7. Wykrycie animacji.
8. Wykrycie kanału alfa.
9. Odczyt metadanych.
10. Wybór ustawień optymalizacji.
11. Dekodowanie obrazu.
12. Obrót zgodny z EXIF.
13. Zmiana wymiarów.
14. Konwersja przestrzeni kolorów.
15. Usunięcie albo zachowanie metadanych.
16. Kodowanie do wybranego formatu.
17. Porównanie wyniku z oryginałem.
18. Wybór najlepszego wariantu.
19. Zapis do pliku tymczasowego.
20. Sprawdzenie poprawności zapisanego pliku.
21. Atomowe przeniesienie do pliku końcowego.
22. Aktualizacja postępu.
23. Zwrócenie wyniku do interfejsu.

Każdy etap powinien:

- obsługiwać błędy;
- umożliwiać anulowanie;
- rejestrować czas wykonania;
- mieć sensowne logowanie;
- mieć test jednostkowy, jeśli jest to uzasadnione.

## WYDAJNOŚĆ

Zoptymalizuj aplikację pod kątem szybkości i niskiego zużycia pamięci.

Wymagania:

- nie ładuj wszystkich plików do pamięci jednocześnie;
- korzystaj z kolejki zadań;
- ogranicz liczbę jednocześnie dekodowanych obrazów;
- używaj workerów;
- domyślnie ustaw liczbę workerów na liczbę rdzeni logicznych minus jeden;
- pozwól użytkownikowi zmienić liczbę workerów;
- nie blokuj wątku interfejsu;
- ogranicz niepotrzebne kopie buforów;
- używaj buforów wielokrotnego użytku, jeśli biblioteka na to pozwala;
- używaj przetwarzania strumieniowego, jeśli jest dostępne;
- dodaj limity dla bardzo dużych obrazów;
- zabezpiecz aplikację przed obrazami powodującymi nadmierne zużycie pamięci;
- obsługuj anulowanie za pomocą tokenu anulowania;
- sprzątaj pliki tymczasowe po anulowaniu;
- mierz czas dekodowania;
- mierz czas skalowania;
- mierz czas kodowania;
- mierz całkowity czas operacji;
- dodaj benchmarki dla różnych rozmiarów plików;
- dodaj benchmarki dla różnych liczb workerów.

Przygotuj benchmarki dla:

- JPEG do WebP;
- JPEG do AVIF;
- PNG do WebP;
- PNG do PNG;
- WebP do AVIF;
- skalowania obrazów;
- pojedynczego pliku;
- dużej paczki plików;
- różnych ustawień jakości;
- różnych liczb workerów.

Nie używaj unsafe, chyba że jest to absolutnie konieczne.

Jeżeli użycie unsafe okaże się konieczne:

- ogranicz jego zakres;
- udokumentuj każdy fragment;
- wyjaśnij, dlaczego jest potrzebny;
- dodaj testy bezpieczeństwa;
- nie ukrywaj tego w kodzie.

## BEZPIECZEŃSTWO

Aplikacja musi:

- poprawnie obsługiwać Unicode;
- chronić się przed path traversal;
- nie ufać rozszerzeniu pliku;
- wykrywać format na podstawie zawartości;
- mieć limit maksymalnego rozmiaru pliku;
- mieć limit maksymalnych wymiarów obrazu;
- bezpiecznie obsługiwać uszkodzone pliki;
- nie nadpisywać oryginałów bez zgody;
- tworzyć bezpieczne pliki tymczasowe;
- usuwać pliki tymczasowe po zakończeniu;
- usuwać pliki tymczasowe po anulowaniu;
- nie wykonywać poleceń systemowych na podstawie danych użytkownika;
- nie uruchamiać zewnętrznych skryptów;
- nie pobierać danych z internetu;
- nie wysyłać telemetrii;
- nie przechowywać kopii oryginalnych plików;
- zwracać kontrolowane błędy zamiast panic;
- obsługiwać brak uprawnień;
- obsługiwać brak miejsca na dysku;
- obsługiwać konflikt nazw;
- obsługiwać przerwanie procesu;
- nie niszczyć oryginału nawet w razie awarii.

Bezpieczny zapis pliku powinien działać następująco:

1. Utwórz bezpieczny plik tymczasowy.
2. Zapisz wynik do pliku tymczasowego.
3. Sprawdź, czy plik można ponownie odczytać.
4. Jeśli to konieczne, wykonaj synchronizację danych.
5. Atomowo przenieś plik tymczasowy do ścieżki końcowej.
6. Usuń plik tymczasowy.

## INTERFEJS UŻYTKOWNIKA

Zaprojektuj nowoczesny, minimalistyczny i profesjonalny interfejs.

Interfejs ma być prosty dla początkujących, ale wystarczająco zaawansowany dla profesjonalistów.

Preferowane cechy:

- dużo wolnej przestrzeni;
- czytelna typografia;
- subtelne animacje;
- wyraźny obszar przeciągania;
- logiczny układ ustawień;
- brak zbędnych dekoracji;
- tryb jasny i ciemny;
- pełna obsługa klawiatury;
- responsywność;
- dobra dostępność;
- czytelne błędy;
- brak agresywnych kolorów.

Ekran główny powinien zawierać:

- nazwę Imago;
- hasło Fast. Local. Smaller images.;
- duży obszar przeciągania plików;
- przycisk Dodaj pliki;
- przycisk Dodaj folder;
- listę zadań;
- ustawienia jakości;
- wybór formatu;
- ustawienia skalowania;
- ustawienia metadanych;
- wybór folderu wyjściowego;
- przycisk Rozpocznij optymalizację;
- przycisk Anuluj;
- podsumowanie wyników;
- przełącznik motywu;
- panel ustawień zaawansowanych.

Dla każdego pliku wyświetl:

- miniaturę;
- nazwę;
- format wejściowy;
- format wyjściowy;
- wymiary;
- rozmiar początkowy;
- rozmiar końcowy;
- procent redukcji;
- czas przetwarzania;
- status;
- komunikat błędu;
- przycisk ponowienia;
- przycisk pokazania pliku w folderze.

Statusy:

- Oczekuje;
- Przetwarzanie;
- Ukończono;
- Pominięto;
- Anulowano;
- Błąd.

## CLI

Dodaj tryb wiersza poleceń.

Nazwa programu: `imago`

Przykładowe użycie:

```
imago input.jpg --output optimized.jpg --format webp --quality 82
imago ./photos --output ./optimized --format auto --quality 82
imago ./photos --output ./optimized --recursive --workers 8
imago ./photos --output ./optimized --width 2400 --remove-metadata
imago ./photos --output ./optimized --format avif --quality 70
imago ./photos --output ./optimized --lossless
imago ./photos --output ./optimized --skip-existing
imago --help
```

CLI powinno obsługiwać:

- pojedyncze pliki;
- foldery;
- podfoldery;
- tryb rekurencyjny;
- format auto;
- jakość;
- tryb bezstratny;
- szerokość;
- wysokość;
- procent skalowania;
- usuwanie metadanych;
- zachowanie metadanych;
- liczbę workerów;
- pomijanie istniejących plików;
- nadpisywanie;
- tryb cichy;
- tryb szczegółowy;
- folder wyjściowy;
- format ZIP;
- kody zakończenia;
- czytelne komunikaty błędów.

CLI i GUI muszą korzystać z tego samego silnika oraz tych samych struktur danych.

## KONFIGURACJA

Zapisuj lokalnie ustawienia użytkownika.

Konfiguracja powinna obejmować:

- ostatnio wybrany format;
- jakość;
- tryb automatyczny;
- liczbę workerów;
- folder wyjściowy;
- zachowanie metadanych;
- zachowanie istniejących plików;
- preferowany motyw;
- ustawienia skalowania;
- ustawienia ZIP;
- limity dużych plików.

Nie zapisuj:

- oryginalnych obrazów;
- zawartości obrazów;
- niepotrzebnej historii plików;
- danych osobowych;
- danych telemetrycznych.

## TESTY

Dodaj:

- testy jednostkowe;
- testy integracyjne;
- testy CLI;
- testy dla każdego formatu;
- testy błędnych plików;
- testy uszkodzonych plików;
- testy przezroczystości;
- testy EXIF;
- testy orientacji;
- testy animacji;
- testy skalowania;
- testy jakości;
- testy trybu bezstratnego;
- testy trybu automatycznego;
- testy anulowania;
- testy wielu workerów;
- testy zapisu atomowego;
- testy konfliktów nazw;
- testy Unicode;
- testy dużych plików;
- testy braku uprawnień;
- testy braku miejsca na dysku;
- testy, że wynik nie jest większy od oryginału w trybie auto;
- testy poprawności wymiarów;
- testy ochrony oryginalnego pliku.

Dodaj benchmarki Criterion dla:

- kodowania;
- dekodowania;
- skalowania;
- kompresji;
- przetwarzania seryjnego;
- różnych ustawień jakości;
- różnych liczb workerów.

## STRUKTURA PROJEKTU (propozycja)

```
imago/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── CONTRIBUTING.md
├── SECURITY.md
├── LICENSE
├── package.json
├── vite.config.ts
├── index.html
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── types/
│   ├── components/
│   │   ├── Dropzone.tsx
│   │   ├── FileList.tsx
│   │   ├── FileCard.tsx
│   │   ├── SettingsPanel.tsx
│   │   ├── ProgressBar.tsx
│   │   ├── SummaryPanel.tsx
│   │   └── ThemeToggle.tsx
│   ├── hooks/
│   ├── services/
│   ├── store/
│   ├── styles/
│   └── utils/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── commands.rs
│       ├── errors.rs
│       ├── config.rs
│       ├── jobs.rs
│       ├── workers.rs
│       ├── filesystem.rs
│       ├── metadata.rs
│       ├── archive.rs
│       ├── cli.rs
│       ├── image_engine/
│       │   ├── mod.rs
│       │   ├── codecs.rs
│       │   ├── pipeline.rs
│       │   ├── resize.rs
│       │   └── detection.rs
│       └── tests/
├── benches/
├── examples/
└── .github/
    └── workflows/
```

Możesz zmienić strukturę, jeśli będzie to technicznie lepsze, ale wyjaśnij każdą istotną zmianę.

## DOKUMENTACJA

Utwórz:

- README.md;
- CONTRIBUTING.md;
- SECURITY.md;
- instrukcję instalacji;
- instrukcję uruchomienia developerskiego;
- instrukcję budowania wersji release;
- instrukcję Windows;
- instrukcję macOS;
- instrukcję Linux;
- dokument architektury;
- opis pipeline'u;
- opis obsługiwanych formatów;
- opis ograniczeń;
- instrukcję CLI;
- instrukcję rozwiązywania problemów;
- plik licencji zgodny z licencjami użytych bibliotek.

README powinien zawierać:

- opis aplikacji Imago;
- listę najważniejszych funkcji;
- instrukcję instalacji;
- instrukcję uruchomienia;
- przykłady CLI;
- informacje o lokalnym przetwarzaniu;
- informacje o wydajności;
- wymagania systemowe;
- informacje o licencji;
- znane ograniczenia.

## BUDOWANIE

Skonfiguruj następujące polecenia:

```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
npm install
npm run tauri dev
npm run tauri build
```

Dodaj GitHub Actions dla:

- Linux;
- Windows;
- macOS;
- testów Rust;
- testów frontendu;
- clippy;
- rustfmt;
- budowania release;
- tworzenia artefaktów;
- sprawdzania zależności.

Użyj rozsądnych ustawień release:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

Jeśli konkretne ustawienie nie jest kompatybilne z wybraną konfiguracją, wybierz bezpieczną alternatywę zapewniającą poprawny build.

Przygotuj instalatory dla:

- Windows;
- macOS;
- Linux.

Nazwa pliku instalacyjnego powinna zawierać: Imago

## ZASADY IMPLEMENTACJI

- Nie twórz pseudokodu, jeśli można napisać działający kod.
- Nie pozostawiaj pustych funkcji.
- Nie zostawiaj nieużywanych modułów.
- Nie ignoruj błędów przez unwrap bez uzasadnienia.
- Nie ukrywaj problemów technicznych.
- Używaj czytelnych nazw.
- Stosuj małe i testowalne funkcje.
- Dokumentuj publiczne API.
- Oddziel logikę UI od logiki przetwarzania.
- Oddziel CLI od silnika.
- Oddziel Tauri od głównej logiki aplikacyjnej.
- Stosuj ustandaryzowane błędy.
- Dbaj o kompatybilność z Windows, macOS i Linux.
- Unikaj zbędnych zależności.
- Nie dodawaj płatnych usług.
- Nie dodawaj chmury.
- Nie dodawaj telemetrii.
- Nie dodawaj reklam.
- Nie kopiuj kodu ani interfejsu istniejących produktów.
- Nie twórz rozwiązania tylko demonstracyjnego.
- Kod ma być przygotowany jako fundament prawdziwej aplikacji produkcyjnej.

## SPOSÓB PRACY

Pracuj etapami.

**ETAP 1 — ARCHITEKTURA.** Przeanalizuj wymagania; zaproponuj architekturę i strukturę katalogów; wskaż ryzyka techniczne; wybierz biblioteki; uzasadnij wybór libvips, image albo alternatywy; wyjaśnij wielowątkowość, anulowanie, bezpieczny zapis i obsługę dużych plików.

**ETAP 2 — MINIMALNY PROJEKT.** Utwórz działający projekt Rust; skonfiguruj Tauri, React, TypeScript; uruchom pustą aplikację Imago; dodaj nazwę Imago do tytułu okna; sprawdź kompilację; napraw wszystkie błędy.

**ETAP 3 — SILNIK OBRAZÓW.** Wykrywanie formatów; odczyt wymiarów; podstawowa kompresja; konwersja; skalowanie; testy; obsługa błędów.

**ETAP 4 — PIPELINE I WIELOWĄTKOWOŚĆ.** Kolejka zadań; workerzy; przetwarzanie seryjne; anulowanie; postęp; bezpieczny zapis; obsługa dużych paczek plików.

**ETAP 5 — INTERFEJS.** Przeciąganie plików; wybór plików i folderów; lista plików; ustawienia; podgląd; pasek postępu; podsumowanie; tryb jasny i ciemny.

**ETAP 6 — FUNKCJE ZAAWANSOWANE.** Tryb automatyczny; metadane; ZIP; konfiguracja; konflikty nazw; animacje; logowanie.

**ETAP 7 — CLI.** Komenda imago; foldery; tryb rekurencyjny; jakość; formaty; skalowanie; workerzy; testy CLI.

**ETAP 8 — TESTY I RELEASE.** Benchmarki; testy bezpieczeństwa; testy integracyjne; GitHub Actions; dokumentacja; wersja release; build na wszystkich platformach; instrukcja instalacji.

**Po każdym etapie:** pokaż, co zostało zaimplementowane; pokaż strukturę zmienionych plików; podaj zawartości nowych plików; podaj polecenia uruchomienia; uruchom albo zasymuluj testy; sprawdź błędy kompilacji; popraw błędy przed przejściem dalej; wyjaśnij ważne decyzje techniczne.

Jeśli ograniczenia środowiska uniemożliwiają uruchomienie konkretnej komendy, napisz o tym jasno i podaj dokładne polecenie, które użytkownik powinien uruchomić lokalnie.

## KRYTERIA AKCEPTACJI

Projekt będzie uznany za gotowy, gdy:

- aplikacja uruchamia się przez `npm run tauri dev`;
- aplikacja nazywa się Imago;
- działa przeciąganie plików;
- działa wybór plików;
- działa wybór folderów;
- działa przetwarzanie wielu plików;
- działa wybór formatu;
- działa ustawianie jakości;
- działa tryb automatyczny;
- działa skalowanie;
- działa usuwanie metadanych;
- działa wybór folderu wyjściowego;
- działa pasek postępu;
- działa anulowanie;
- działa ponowienie zadania;
- działa pobieranie wyników;
- działa tworzenie ZIP;
- oryginały nie są nadpisywane bez zgody;
- błędne pliki są bezpiecznie obsługiwane;
- aplikacja nie zawiesza się przy dużej liczbie plików;
- działa CLI;
- przechodzą testy;
- clippy nie zgłasza ostrzeżeń;
- rustfmt przechodzi;
- działa build release;
- działa dokumentacja;
- możliwe jest dalsze rozwijanie projektu.

Nazwa aplikacji, interfejsu, CLI i instalatora musi brzmieć: **Imago**
