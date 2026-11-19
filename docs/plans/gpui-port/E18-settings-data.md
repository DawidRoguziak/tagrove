# E18: Ustawienia: import/eksport, przywracanie i duplikaty

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E17.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Operacje na danych z tymi samymi zabezpieczeniami co w Tauri.

## Zakres

- Eksport/import CSV tagów; kopia i przywracanie paczki bazy: inspekcja paczki, mapowanie rootów, potwierdzenie nadpisania; reset stanu tagów po przywróceniu.
- Duplikaty: skan, dialog rozstrzygania (karty grup i zasobów), walidacja, zastosowanie partii z `scanRevision`, potwierdzenie usunięcia.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/settings/createImportExportSettings.ts`
- `src/features/settings/ImportDbOverwriteConfirmDialog.tsx`
- `src/features/settings/createDuplicateSettings.ts`
- `src/features/settings/createDuplicateResolver.ts`
- `src/features/settings/duplicateValidationService.ts`
- `src/features/settings/DuplicateResolverDialog.tsx`
- `src/features/settings/DuplicateGroupCard.tsx`
- `src/features/settings/DuplicateAssetCard.tsx`
- `src/features/settings/DuplicateDeleteConfirmDialog.tsx`
- `src/features/settings/sections/ImportExportSection.tsx`
- `src/features/settings/sections/DuplicatesSection.tsx`

Testy do przeniesienia:
- `src/features/settings/__tests__`

Dokumenty i reguły:
- `docs/subsystems/data-safety-and-portability.md`
- `docs/subsystems/settings-operations.md`
- `.claude/rules/data-safety.md`

## Kryteria akceptacji

- Testy modeli importu/eksportu i duplikatów przeniesione.
- Każdy test niszczący działa na świeżym katalogu tymczasowym i tymczasowej bazie.
- Przywracanie i import CSV przechodzą przez barierę mutacji tagów.
- Parytet `settings-import-export`, `settings-duplicates`, `duplicates-resolver`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E18 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `settings-import-export`: Sekcja importu/eksportu.
- `settings-duplicates`: Sekcja duplikatów po skanie z wynikami.
- `duplicates-resolver`: Dialog rozstrzygania duplikatów z jedną grupą.

## Ryzyka

- Najwyższe ryzyko utraty danych w całym porcie: przegląd z soczewką stabilności obowiązkowy.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
