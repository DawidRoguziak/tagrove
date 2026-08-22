# Etap 10 - Końcowa weryfikacja integralności

## Metryka

- Priorytet: wysoki
- Status: Ukończony
- Zależności: Etapy 00-09
- Następny etap: brak

## Cel

Potwierdzić, że wszystkie wcześniejsze etapy tworzą jeden spójny system, nie przywróciły wcześniejszych wyścigów i mają kompletną dokumentację zachowania oraz recovery.

## Końcowe zaakceptowane ryzyka

- Operacje na źródłach kanonizują istniejącą ścieżkę i przypisane rooty przed stagingiem, ale między sprawdzeniem a operacją ścieżkową pozostaje systemowe okno TOCTOU. Pełne usunięcie wymagałoby operacji względem uchwytu katalogu, na przykład `openat2` na Linux, oraz odpowiednika na Windows.
- `quick-xml 0.38.4`, wciągany wyłącznie przez `plist 1.8.0` i Tauri, ma `RUSTSEC-2026-0194` oraz `RUSTSEC-2026-0195`. Aplikacja nie przyjmuje XML jako danych użytkownika, aktualne Tauri/`plist` nie dopuszcza `quick-xml >=0.41`, więc oba identyfikatory są jawnymi wyjątkami bramki do czasu aktualizacji upstream. Pozostałe ostrzeżenia RustSec dotyczą transitive, target-specific lub nieutrzymywanych zależności Tauri/image; bramka nadal raportuje je informacyjnie.
- Pełny `bun audit` nadal zgłasza wysokie advisory w dev-only drzewie WebdriverIO (`deepmerge-ts`, `extract-zip`, `serialize-javascript`), mimo aktualizacji do najnowszych zgodnych wersji. `bun audit --production --audit-level=high` jest bramką i przechodzi bez podatności produkcyjnych.
- Canonical confinement nie zastępuje praw dostępu systemu operacyjnego, a trwałość rename/sync zależy od gwarancji hostowego filesystemu. Thumbnail cleanup pozostaje best effort i może zostawić niepowiązane pliki.
- Nie wykonano zainstalowanego smoke testu MSI/NSIS, pełnego Docker release build, uruchomienia workflow na GitHubie, ręcznego audytu screen reader/kontrastu/zoomu ani przeglądu wszystkich ekranów przez native speakerów 11 języków. Te kontrole pozostają bramkami wydania/platformy, nie blockerem spójności kodu.

## Zadania audytowe

- [x] Sprawdzić wszystkie kryteria odbioru Etapów 00-09.
- [x] Porównać typy TypeScript z modelami Rust i rzeczywistą serializacją IPC.
- [x] Porównać wrappery `src/api.ts` z listą `generate_handler!` i usunąć niepotrzebne legacy endpoints.
- [x] Sprawdzić kolejność maintenance, scan i thumb locków we wszystkich workflow.
- [x] Sprawdzić atomowość mutacji i revision dla scan, delete, rename, tag, favorite, group, CSV i restore.
- [x] Sprawdzić confinement każdej ścieżki używanej do usuwania lub nadpisywania pliku.
- [x] Sprawdzić recovery po każdym commit point operacji filesystem plus DB.
- [x] Sprawdzić invalidation query, summary, details, selection i thumbnail cache.
- [x] Sprawdzić limity ZIP, obrazów, kolejek, połączeń i timeoutów.
- [x] Potwierdzić zachowanie basename-only i fan-out CSV.
- [x] Potwierdzić kompletność locale, placeholderów i dostępności modalnej.
- [x] Porównać dokumentację architektury, IPC, bazy, backupu i testów z kodem.
- [x] Zaktualizować trwałe reguły w `AGENTS.md` tylko wtedy, gdy są rzeczywiście ogólnoprojektowe; audyt nie wykazał potrzeby zmiany.
- [x] Zamknąć albo jawnie zaakceptować każde pozostałe ryzyko.

## Planowana walidacja automatyczna

Poniższe komendy można uruchomić wyłącznie po osobnym, wyraźnym poleceniu:

```bash
bun install --frozen-lockfile
bun run build
bun run test
bun run test:backend
bun run test:e2e:tauri
```

Dodatkowo należy uruchomić zaakceptowane w Etapie 09 kontrole lint, format, locale, Rust fmt/clippy i advisory. Dla pakietu Windows potrzebny jest osobny smoke test zainstalowanego MSI/NSIS i sidecarów. Dla Linux release należy użyć udokumentowanego workflow Docker.

## Kryteria odbioru

- Każdy etap ma status, daty, dziennik i rozliczone kryteria.
- Nie istnieje mutacja widoczna dla query bez atomowego revision lub jawnie udokumentowanej przyczyny.
- Żadna niezaufana ścieżka nie może sterować usuwaniem poza dozwolonym rootem.
- Backup i restore mają potwierdzoną spójność oraz recovery.
- Frontend i backend zgadzają się na wszystkich publicznych kontraktach.
- Wszystkie autoryzowane bramki weryfikacyjne przechodzą albo mają opisany blocker.
- `README.md` wskazuje brak kolejnego etapu i końcowy zestaw zaakceptowanych ryzyk.

## Dziennik

### 2026-08-22 - rozpoczęcie audytu

- Status po sesji: W toku.
- Zmienione pliki: `docs/integration_fix_plans/README.md`, ten dokument i metryka Etapu 05.
- Wdrożone zachowanie: przywrócono zatwierdzony stan Etapów 07-09 po wykryciu roboczego odwrócenia ich zmian; Etap 10 jest jedynym etapem w toku.
- Decyzje i odchylenia: wszystkie odwrócone pliki należały do zakresu commitów Etapów 07-09; nieśledzone artefakty wskazane przez użytkownika pozostały nietknięte.
- Migracje i kompatybilność: brak.
- Utworzone lub zmienione testy: audyt i testy jeszcze trwają.
- Uruchomione sprawdzenia: porównanie stanu roboczego z zakresem commitów Etapów 07-09.
- Niewykonane sprawdzenia i ryzyka: wszystkie zadania audytowe i bramki Etapu 10 pozostają do wykonania.
- Następny dokładny krok: porównać kontrakty, locki, transakcje, ścieżki, recovery, cache i limity z kodem oraz testami.

### 2026-08-22 - zakończenie audytu i remediation

- Status po sesji: Ukończony.
- Zmienione pliki: kontrakty IPC i modele TypeScript/Rust; rejestracja komend i wrappery API; backend DB, skanowania, restore, CSV, mutacji plikowych i miniaturek; frontend cache, Shift-selection, details i lightbox; testy regresyjne; lockfile, quality gates, CI i dokumentacja subsystemów.
- Wdrożone zachowanie: usunięto trzy nieużywane legacy endpointy; uściślono `LegacyAsset`, wymagane `ScanSummary.completion` i bezpieczny liczbowo payload duplikatów; atomowo powiązano destructive scan prune i clear-library z revision; ograniczono kanał wyników skanu, waiterów miniaturek i wejście CSV; dodano confinement źródeł i miniaturek oraz Unix directory sync; naprawiono restore mapowań z oboma separatorami; usunięto stale summary/details i Shift-range races; nested delete modal blokuje skróty lightboxa.
- Decyzje i odchylenia: `list_assets` pozostaje publiczne, bo desktop E2E używa go do bezpośrednich asercji workflow. `AGENTS.md` nie zmieniono, ponieważ audyt nie odkrył nowej reguły ogólnoprojektowej. Zależności JS zaktualizowano do najnowszych zgodnych wersji, a `crossbeam-epoch` do `0.9.20`.
- Migracje i kompatybilność: brak migracji bazy i danych. Usunięte komendy nie miały konsumentów aplikacji; `list_assets` zachowuje kontrakt legacy. Usunięcie `fingerprint_mtime_ns` dotyczy tylko payloadu skanu duplikatów, nie kolumny ani wewnętrznej wersji źródła.
- Utworzone lub zmienione testy: regresje transakcji prune/revision, symlink escape, thumbnail confinement, restore mapping, limity CSV i waiterów; frontendowe regresje replacement cache, kompletnych ID ranges, stale Shift request, filtered-tag refresh, details cache i nested-modal keyboard handling.
- Uruchomione sprawdzenia: po końcowym review skupione Vitest 87/87 i testy usług Rust 12/12; typecheck; rustfmt; locked Clippy z `-D warnings`; produkcyjny Bun audit; RustSec z dwoma opisanymi wyjątkami; końcowe pełne `test:all`: Vitest 354/354 w 61 plikach, Rust 137 + 1 + 6, desktop E2E 13/13 w 2 plikach spec. Pierwsza końcowa próba wyczerpała miejsce podczas linkowania w tymczasowym `CARGO_TARGET_DIR`; po wyczyszczeniu wyłącznie tego cache dokładnie ta sama komenda przeszła. `git diff --check` nie zgłasza błędów.
- Niewykonane sprawdzenia i ryzyka: wyłącznie kontrole platformowe/manualne i zaakceptowane ryzyka wymienione powyżej.
- Następny dokładny krok: monitorować wyjątki advisory i wykonać smoke testy pakietów na docelowych platformach przed wydaniem; kolejnego etapu integracyjnego brak.
