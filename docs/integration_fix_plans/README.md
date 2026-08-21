# Plan napraw integralności projektu

Ten katalog jest nadrzędnym źródłem prawdy dla prac nad integralnością MediaTagger. Dokument `docs/plans/04-deep-review-remediation-plan.md` pozostaje zapisem historycznym i źródłem wcześniejszych decyzji, ale postęp dalszych prac należy aktualizować tutaj.

## Stan planu

- Ostatnia aktualizacja: 2026-08-21
- Aktualny etap: 03 ukończony
- Następny krok: w nowej sesji rozpocząć wyłącznie Etap 04 od ponownego odczytu jego planu i aktualnego kodu sesji zapytań/invalidation
- Otwarte blokery: brak zgody na tworzenie, aktualizowanie i uruchamianie testów oraz buildów
- Ukończony wcześniej fundament: bezpieczny częściowy skan i blokada pojedynczej instancji profilu

| Etap | Dokument | Zakres | Priorytet | Status |
| --- | --- | --- | --- | --- |
| 00 | [Stan roboczy](00-current-worktree-reconciliation.md) | Uporządkowanie bieżących zmian i zabezpieczenie asynchronicznego media servera | Wysoki | Ukończony |
| 01 | [Usuwanie i duplikaty](01-safe-deletion-and-duplicates.md) | Bezpieczne usuwanie plików i atomowe rozwiązywanie duplikatów | Krytyczny | Ukończony |
| 02 | [Walidacja importu backupu](02-backup-import-validation.md) | Ograniczenie zaufania do ZIP, SQLite i importowanych ścieżek | Krytyczny | Ukończony |
| 03 | [Spójny backup i restore](03-consistent-backup-and-restore.md) | Snapshot SQLite, maintenance lock i odzyskiwalna podmiana danych | Krytyczny | Ukończony |
| 04 | [Zapytania i invalidation](04-query-sessions-and-invalidation.md) | Generacje zapytań, revision, membership i kolejność galerii | Wysoki | Oczekuje |
| 05 | [Selekcja i cache](05-selection-cache-and-details.md) | Globalne indeksy, summary/details, cache epoch i Shift-zaznaczenie | Wysoki | Oczekuje |
| 06 | [Miniatury i praca w tle](06-thumbnails-and-background-workers.md) | Wersjonowanie miniaturek, scheduler, timeouty i awarie workerów | Wysoki | Oczekuje |
| 07 | [CSV, tagi i revision](07-csv-tags-and-revision-atomicity.md) | Atomowy import CSV i jeden invariant tagów | Wysoki | Oczekuje |
| 08 | [Błędy, modale i dostępność](08-modals-errors-and-accessibility.md) | Retry, stos warstw, Escape, fokus i ARIA | Średni | Oczekuje |
| 09 | [Lokalizacja i quality gates](09-localization-and-quality-gates.md) | Kompletność locale, generator, CI i przypięcie narzędzi | Średni | Oczekuje |
| 10 | [Weryfikacja końcowa](10-final-integrity-verification.md) | Audyt kontraktów, danych, konfiguracji i pełna walidacja | Wysoki | Oczekuje |

## Wiążące decyzje

1. CSV identyfikuje zasoby wyłącznie po nazwie pliku. Fan-out na wszystkie rekordy o tej samej nazwie jest zamierzoną semantyką.
2. Utwardzanie CSP, globalnego `assetProtocol` i SmartScreen pozostaje poza zakresem tego planu, dopóki nie zapadnie osobna decyzja.
3. Bez osobnego, wyraźnego polecenia nie wolno tworzyć ani aktualizować testów oraz uruchamiać testów, buildów i E2E.
4. Nie wolno tworzyć dodatkowego Git worktree.
5. Kod i konfiguracja wykonywalna są źródłem prawdy. Dokumentację aktualizuje się razem ze zmianą zachowania.
6. SQL pozostaje w `src-tauri/src/db.rs`, workflow w `src-tauri/src/services/*`, a komendy Tauri powinny być cienkimi adapterami.
7. Jeżeli operacja wymaga `scan_lock` i zapisu `thumb_lock`, obowiązuje kolejność `scan_lock`, następnie `thumb_lock`.
8. Mutacja widoczna w zapytaniach i odpowiadający jej bump revision powinny należeć do tej samej transakcji.

## Statusy

- `Oczekuje` - etap nie został rozpoczęty.
- `W toku` - etap ma właściciela i trwają nad nim prace.
- `Zablokowany` - dalsza praca wymaga konkretnej decyzji lub zależności.
- `Ukończony` - wszystkie zadania i kryteria odbioru zostały spełnione, a niewykonane sprawdzenia są jawnie zapisane.

## Protokół aktualizacji

1. Przed zmianą kodu ustawić dokładnie jeden etap na `W toku` i zaktualizować tabelę w tym pliku.
2. Przeczytać dokument etapu, wskazane dokumenty architektury oraz aktualny kod.
3. Oznaczać wykonane zadania bez usuwania pierwotnego zakresu.
4. Nowe ryzyko dopisać do właściwego etapu albo utworzyć jawny follow-up w Etapie 10.
5. Po sesji uzupełnić datę, zmienione pliki, zachowanie, decyzje, migracje, niewykonane sprawdzenia i następny krok.
6. Status `Ukończony` ustawić dopiero po spełnieniu kryteriów odbioru, nie po samym zakończeniu implementacji.

## Potwierdzony stan bazowy

- `git fsck` nie wykazał uszkodzeń obiektów repozytorium.
- Główne pliki JSON przechodzą parsowanie.
- `git diff --check` nie wykazał błędów whitespace.
- Nie ma nierozwiązanych konfliktów Git.
- Warstwy testowe istnieją dla frontendu, Rust i desktop E2E, ale repozytorium nie ma workflow CI.
- Bun, Node, Rust i Cargo nie są przypięte na poziomie repozytorium.
- Zmiany bazowe obejmujące media server, nawigację lightboxa, cache stron i E2E zostały sklasyfikowane i rozliczone w Etapie 00.

## Szablon dziennika etapu

```md
### YYYY-MM-DD - sesja implementacyjna

- Status po sesji: W toku / Ukończony / Zablokowany
- Zmienione pliki: ...
- Wdrożone zachowanie: ...
- Decyzje i odchylenia: ...
- Migracje i kompatybilność: ...
- Utworzone lub zmienione testy: nie wykonywano bez osobnego polecenia / ...
- Uruchomione sprawdzenia: nie wykonywano bez osobnego polecenia / ...
- Niewykonane sprawdzenia i ryzyka: ...
- Następny dokładny krok: ...
```
