# Współtworzenie Imago

Dziękujemy za chęć pomocy! Imago jest wolnym oprogramowaniem (GPL-3.0-or-later). Zgłaszając zmiany, zgadzasz się
na udostępnienie ich na tej samej licencji.

## Zanim zaczniesz

- Przeczytaj [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) i [docs/DECISIONS.md](docs/DECISIONS.md).
- Środowisko: [docs/INSTALL.md](docs/INSTALL.md) (Rust stable ≥ 1.93, Node 22, kompilator C, NASM).
- Stan prac i plan: [docs/PROGRESS.md](docs/PROGRESS.md), macierz wymagań: [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md).

## Układ repozytorium

| Katalog | Zawartość |
|---|---|
| `crates/imago-core` | silnik (biblioteka): detekcja, dekodowanie, kodeki, pipeline, workerzy, zapis, ZIP, konfiguracja |
| `crates/imago-cli` | binarka `imago` |
| `src-tauri` | adapter Tauri — tylko komendy IPC, stan, logowanie |
| `src` | interfejs React + TypeScript |
| `docs` | dokumentacja projektowa i użytkowa (po polsku) |

Logika przetwarzania należy wyłącznie do `imago-core`. GUI i CLI nie przetwarzają obrazów samodzielnie.

## Zasady kodu

- Kod, nazwy i komentarze w kodzie po angielsku; dokumenty w `docs/` po polsku.
- `#![forbid(unsafe_code)]` obowiązuje w całym projekcie. Nowy kod `unsafe` wymaga ADR, uzasadnienia i testów.
- Żadnych `unwrap()`/`expect()` na danych zewnętrznych (clippy `unwrap_used`, `expect_used`); błędy przez `ImagoError`
  ze stabilnym `code()`. Nowy kod błędu = nowy tekst w `src/i18n/pl.ts` i `en.ts` (test to sprawdza).
- Oryginał nigdy nie jest otwierany do zapisu; każdy zapis przez `filesystem::safe_write`.
- Brak kodu sieciowego, telemetrii, uruchamiania procesów na podstawie danych użytkownika.
- Publiczne API `imago-core` jest dokumentowane (`#![warn(missing_docs)]`).
- Nowe zależności muszą być zgodne z GPL-3.0 (lista dozwolonych licencji w `deny.toml`); zaktualizuj `THIRD_PARTY.md`.
- Interfejs: tylko CSS z tokenami (`src/styles/tokens.css`), obsługa klawiatury, etykiety dostępności, oba motywy,
  oba języki.

## Przed wysłaniem zmian

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy -p imago-core --no-default-features --all-targets -- -D warnings
cargo test --all
npm run typecheck
npm test
```

Każda poprawka błędu powinna mieć test, który bez niej nie przechodzi. Testy tworzą obrazy w kodzie
(`crates/imago-core/tests/common`) — nie dodawaj binarnych plików testowych bez potrzeby.
Przy zmianach wydajności dołącz wyniki `cargo bench -p imago-core` przed i po.

## Zgłaszanie błędów

Opisz system, wersję Imago (`imago --version` lub nagłówek aplikacji), kroki, oczekiwany i faktyczny wynik oraz
fragment logu (zob. [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)). Nie dołączaj prywatnych zdjęć — jeśli problem
dotyczy konkretnego pliku, spróbuj go odtworzyć na obrazie, który możesz udostępnić.

Podatności bezpieczeństwa zgłaszaj zgodnie z [SECURITY.md](SECURITY.md), nie publicznie.
