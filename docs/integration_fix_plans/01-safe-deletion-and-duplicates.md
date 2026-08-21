# Etap 01 - Bezpieczne usuwanie i atomowe rozwiązywanie duplikatów

## Metryka

- Priorytet: krytyczny
- Status: Ukończony
- Zależności: Etap 00
- Następny etap: 02

## Cel

Zapewnić, że błąd operacji plikowej nie powoduje cichej utraty metadanych, a batch rozwiązywania duplikatów nie pozostawia nieopisanego pół-stanu.

## Potwierdzone ryzyka

- `delete_asset` usuwa rekord DB i bumpuje revision przed próbą usunięcia źródła.
- `removed_media_file: false` łączy brak pliku z błędem uprawnień lub I/O.
- Lightbox i resolver duplikatów ignorują podsumowanie usunięcia.
- Resolver wykonuje sekwencję niezależnych rename/delete; wcześniejsze operacje pozostają po późniejszym błędzie.
- Rename używa `exists()` przed `fs::rename`, co nie daje atomowego no-clobber.
- Błędy kompensacyjnego rename są ignorowane.

## Zadania

- [x] Zdefiniować commit point i kolejność walidacji, operacji plikowej, transakcji DB, bumpu revision i kompensacji.
- [x] Wprowadzić wynik rozróżniający usunięcie, brak źródła i błąd systemu plików.
- [x] Nie usuwać metadanych po nieudanym usunięciu istniejącego pliku bez jawnego, odzyskiwalnego mechanizmu stagingu.
- [x] Rozważyć przeniesienie pliku do kontrolowanego stagingu na tym samym filesystemie przed transakcją DB.
- [x] Wykonać mutację DB i bump revision w jednej transakcji.
- [x] Przenieść workflow usuwania i rename z komendy do serwisu.
- [x] Zaprojektować jeden kontrakt batch dla duplikatów z oczekiwanymi ID, ścieżkami i wersjami rekordów.
- [x] Walidować cały batch oraz kolizje nazw przed pierwszą zmianą.
- [x] Wykonać batch pod wspólnymi lockami i raportować wynik każdej pozycji.
- [x] Zabezpieczyć rename przed nadpisaniem celu i rozszerzyć walidację nazw Windows.
- [x] Zmienić frontend tak, aby honorował wynik częściowy i nie pokazywał fałszywego sukcesu.

## Kryteria odbioru

- Błąd usunięcia istniejącego pliku nie usuwa bezpowrotnie jego metadanych.
- UI odróżnia pełny sukces, brak źródła i błąd plikowy.
- Batch duplikatów jest walidowany w całości przed mutacją.
- Niepowodzenie batcha ma kontrolowany rollback albo precyzyjny, możliwy do wznowienia wynik.
- Revision odpowiada dokładnie zatwierdzonej mutacji DB.

## Planowana weryfikacja

- Fault injection dla permission denied, missing file, rename collision, DB failure i compensation failure.
- Integracja na tymczasowym filesystemie i bazie.
- Desktop E2E reprezentatywnego batcha po uzyskaniu osobnej zgody.

## Dziennik

### 2026-08-21 - rozpoczęcie implementacji

- Status po sesji: W toku
- Zmienione pliki: `docs/integration_fix_plans/README.md`, `docs/integration_fix_plans/01-safe-deletion-and-duplicates.md`
- Wdrożone zachowanie: etap przejęty do realizacji po sprawdzeniu stanu repozytorium, dokumentacji architektury i aktualnych przepływów usuwania, rename oraz resolvera duplikatów.
- Decyzje i odchylenia: commit point będzie następował po bezpiecznym stagingu źródła i w jednej transakcji obejmującej mutację DB oraz revision; batch będzie walidowany w całości i wykonywany przez jeden kontrakt IPC pod wspólnymi lockami.
- Migracje i kompatybilność: do ustalenia podczas implementacji kontraktu wersji rekordów.
- Utworzone lub zmienione testy: nie wykonywano bez osobnego polecenia.
- Uruchomione sprawdzenia: odczyt `git status` i `git log`; nie uruchamiano testów ani buildów.
- Niewykonane sprawdzenia i ryzyka: testy, buildy i E2E pozostają zabronione bez osobnej zgody.
- Następny dokładny krok: wdrożyć backendowy serwis mutacji plików, transakcyjne helpery DB i kontrakty IPC.

### 2026-08-21 - zakończenie implementacji

- Status po sesji: Ukończony
- Zmienione pliki: backend `src-tauri/src/{commands/assets.rs,db.rs,lib.rs,models.rs,services/asset_mutation_service.rs,services/mod.rs}`, kontrakt Cargo, frontend API/types/lightbox/resolver/i18n oraz dokumentacja architektury i podsystemów powiązana z Etapem 01.
- Wdrożone zachowanie: same-filesystem staging i trwały journal z transakcyjną flagą commit; CAS po ID/path/`record_version`; jedna transakcja mutacji i revision; atomowy no-clobber na Linux/Windows; pełna walidacja batcha przed zmianą; reverse rollback i per-item recovery paths; startup reconciliation; strukturalne wyniki delete/rename/batch; zgodne komunikaty lightboxa i resolvera.
- Decyzje i odchylenia: rename source-target cycles są jawnie odrzucane, aby rollback był deterministyczny; cleanup-only błąd zatwierdzonego delete zachowuje journal, ale nie blokuje startu; niepewna zgodność aktywnego rekordu z plikiem blokuje start. Pojedyncze delete/rename korzystają z tego samego serwisu, ale wymóg rozwiązania całej grupy dotyczy tylko batcha resolvera.
- Migracje i kompatybilność: `assets.record_version INTEGER NOT NULL DEFAULT 1`; nowa tabela `pending_file_operations` przechowuje ścieżki oraz `committed`; kontrakty `DeleteAssetSummary`, `RenameAssetSummary`, `DuplicateAsset` i `DuplicateScanSummary` rozszerzono, dodano `apply_duplicate_resolution_batch`.
- Utworzone lub zmienione testy: nie wykonywano bez osobnego polecenia; zgodnie z zakazem istniejących testów nie modyfikowano.
- Uruchomione sprawdzenia: `git diff --check`; parsowanie wszystkich `src/i18n/locales/*.json` przez `jq`; trzy rundy statycznego review kodu i kontraktów. Próby `cargo fmt --check` i parsera przez `node` nie wystartowały, ponieważ oba programy są niedostępne w środowisku.
- Niewykonane sprawdzenia i ryzyka: nie uruchamiano testów, buildów ani E2E; fault injection dla permission denied, DB failure, rollback failure i post-commit cleanup pozostaje niewykonany do czasu osobnej zgody. Kompilacja Rust/TypeScript nie została potwierdzona narzędziowo.
- Następny dokładny krok: zakończyć sesję po osobnym commicie Etapu 01; Etap 02 rozpocząć dopiero w nowej sesji po ponownym odczycie jego planu.
