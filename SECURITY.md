# Bezpieczeństwo

## Zgłaszanie podatności

Nie zgłaszaj podatności w publicznych zgłoszeniach. Użyj prywatnego zgłoszenia (*Security → Report a vulnerability*
w repozytorium) albo skontaktuj się z opiekunami projektu prywatnie. Podaj wersję, system, opis, kroki odtworzenia
i — jeśli to plik — sposób jego wygenerowania. Odpowiadamy możliwie szybko i uzgadniamy termin publikacji poprawki.

Wspierana jest najnowsza wydana wersja.

## Model zagrożeń

Imago przetwarza **niezaufane pliki** (obrazy z internetu, od innych osób). Zakładamy, że plik może być celowo
spreparowany. Cele:

1. Żaden plik nie może spowodować wykonania kodu, zawieszenia ani niekontrolowanego zużycia pamięci.
2. Żaden plik ani ustawienie nie może spowodować zapisu poza wybranym folderem ani zniszczenia oryginału.
3. Żadne dane nie opuszczają komputera.

## Zabezpieczenia

- **Brak sieci i telemetrii.** Backend nie zawiera kodu sieciowego; okno ma restrykcyjne CSP (`connect-src ipc:`,
  brak zewnętrznych skryptów); brak automatycznych aktualizacji.
- **Minimalne uprawnienia webview.** Interfejs ma tylko `core:default`. Dialogi plików działają po stronie Rusta;
  miniatury, podgląd, „pokaż w folderze” i eksport są dozwolone wyłącznie dla ścieżek dodanych przez użytkownika
  lub zapisanych przez Imago w bieżącej sesji (rejestr ścieżek) — webview nie może czytać dowolnych plików.
- **Wykrywanie formatu po zawartości**, nie po rozszerzeniu.
- **Limity przed dekodowaniem**: rozmiar pliku, wymiary, liczba pikseli, łączna liczba pikseli animacji; limity alokacji
  w dekoderach; budżet pamięci.
- **Izolacja błędów**: każdy plik w `catch_unwind` (`panic = "unwind"`, ADR-004), osobne osłony dla parserów i koderów.
  Testy z losowymi danymi i odwróconymi bajtami nagłówków potwierdzają brak paniki (`tests/security.rs`).
- **Bezpieczny zapis**: plik tymczasowy w katalogu docelowym, weryfikacja, `fsync`, atomowe przeniesienie bez nadpisania;
  oryginał nigdy nie jest otwierany do zapisu, nadpisanie oryginału wymaga jawnej zgody.
- **Ścieżki**: wyjście budowane tylko z normalnych komponentów, sprawdzenie, że leży w folderze docelowym; sanityzacja
  przyrostka, nazwy podfolderu i nazw wpisów ZIP; skaner nie wchodzi w dowiązania symboliczne do katalogów.
- **Brak `unsafe`** w kodzie projektu (`#![forbid(unsafe_code)]`). Kod natywny (MozJPEG, libwebp, libdeflate) i `unsafe`
  w zależnościach pochodzą z utrzymywanych bibliotek; zależności są sprawdzane przez `cargo deny` i `npm audit` w CI.
- **Brak uruchamiania poleceń**: aplikacja nie wywołuje powłoki ani programów zewnętrznych na podstawie danych
  użytkownika (folder otwierany jest przez API systemu).

## Znane ryzyka

- Dekodery obrazów to złożony kod; mimo osłon błąd w bibliotece natywnej mógłby wpłynąć na proces. Aktualizuj Imago.
- Instalatory nie są podpisane cyfrowo — pobieraj je wyłącznie z oficjalnych wydań i porównuj sumy kontrolne.
