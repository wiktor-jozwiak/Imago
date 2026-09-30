# Podpisywanie instalatorów

Niepodpisany instalator działa, ale Windows SmartScreen („System Windows ochronił ten komputer”) i macOS Gatekeeper
ostrzegają przed nim. Podpis wymaga certyfikatu wydanego na właściciela projektu — tego nie da się zrobić w kodzie.
Workflow `release.yml` jest przygotowany: **podpisuje automatycznie, gdy w repozytorium GitHub są odpowiednie sekrety**
(*Settings → Secrets and variables → Actions*). Bez nich buduje instalatory bez podpisu, jak dotąd (ADR-019).

Niezależnie od podpisu każde wydanie ma plik `SHA256SUMS.txt` z sumami kontrolnymi wszystkich plików.

> Ceny, wymagania weryfikacyjne i dostępność usług zmieniają się — przed zakupem sprawdź aktualne warunki u dostawcy.

## Windows — Azure Trusted Signing (zalecane)

Usługa Microsoftu: certyfikat jest przechowywany i odnawiany w chmurze, bez sprzętowego tokenu. Wymaga subskrypcji
Azure i weryfikacji tożsamości (osoby lub firmy).

1. W portalu Azure utwórz konto **Trusted Signing**, przejdź weryfikację tożsamości i utwórz **profil certyfikatu**
   (Public Trust).
2. Utwórz rejestrację aplikacji (Microsoft Entra ID) z sekretem i nadaj jej rolę *Trusted Signing Certificate Profile
   Signer* na koncie Trusted Signing.
3. Dodaj sekrety w GitHubie:

   | Sekret | Wartość |
   |---|---|
   | `AZURE_TENANT_ID` | identyfikator katalogu (tenant) |
   | `AZURE_CLIENT_ID` | identyfikator aplikacji |
   | `AZURE_CLIENT_SECRET` | sekret aplikacji |
   | `TRUSTED_SIGNING_ENDPOINT` | np. `https://weu.codesigning.azure.net/` (region konta) |
   | `TRUSTED_SIGNING_ACCOUNT` | nazwa konta Trusted Signing |
   | `TRUSTED_SIGNING_PROFILE` | nazwa profilu certyfikatu |

Workflow instaluje `trusted-signing-cli` i przekazuje go Tauri jako `signCommand` — podpisywane są `imago-app.exe`,
instalatory MSI/NSIS oraz `imago.exe` z archiwum CLI.

## Windows — certyfikat w pliku `.pfx`

Dla starszych certyfikatów OV, które można wyeksportować do pliku (od 2023 r. nowe certyfikaty mają klucz w sprzętowym
module i nie da się ich tak użyć).

| Sekret | Wartość |
|---|---|
| `WINDOWS_CERTIFICATE` | zawartość pliku `.pfx` w Base64 (`[Convert]::ToBase64String([IO.File]::ReadAllBytes("cert.pfx"))`) |
| `WINDOWS_CERTIFICATE_PASSWORD` | hasło do pliku `.pfx` |

Certyfikat jest importowany do magazynu użytkownika na maszynie CI, a Tauri podpisuje przez `signtool` (SHA-256,
znacznik czasu DigiCert). Jeżeli skonfigurowano też Trusted Signing, ma on pierwszeństwo.

## macOS — Developer ID i notaryzacja

Wymaga płatnego członkostwa w Apple Developer Program.

| Sekret | Wartość |
|---|---|
| `APPLE_CERTIFICATE` | certyfikat *Developer ID Application* wyeksportowany do `.p12`, w Base64 |
| `APPLE_CERTIFICATE_PASSWORD` | hasło do `.p12` |
| `APPLE_SIGNING_IDENTITY` | np. `Developer ID Application: Jan Kowalski (TEAMID)` |
| `APPLE_ID` | e-mail konta Apple |
| `APPLE_PASSWORD` | hasło aplikacji (app-specific password) do notaryzacji |
| `APPLE_TEAM_ID` | identyfikator zespołu |

`tauri-action` podpisuje aplikację, wysyła ją do notaryzacji i dołącza wynik (staple) do `.dmg`.

## Podpisywanie lokalne (Windows)

Aby podpisać build zrobiony na własnym komputerze, utwórz plik `src-tauri/tauri.signing.json` (jest w `.gitignore`):

```json
{ "bundle": { "windows": { "signCommand": "trusted-signing-cli -e <ENDPOINT> -a <KONTO> -c <PROFIL> -d Imago %1" } } }
```

albo — dla certyfikatu zainstalowanego w systemie —

```json
{ "bundle": { "windows": { "certificateThumbprint": "<ODCISK>", "digestAlgorithm": "sha256", "timestampUrl": "http://timestamp.digicert.com" } } }
```

i zbuduj:

```bash
npm run tauri build -- --config src-tauri/tauri.signing.json
```

## Reputacja SmartScreen

Podpis identyfikuje wydawcę, ale SmartScreen buduje reputację stopniowo — pierwsze wydania z nowym certyfikatem mogą
jeszcze wyświetlać ostrzeżenie, dopóki instalator nie zostanie pobrany odpowiednio wiele razy.
