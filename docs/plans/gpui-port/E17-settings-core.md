# E17: Ustawienia: szkielet, skan, miniatury, wygląd, język, strefa zagrożeń, o programie

Rozmiar: L. Poziom: standardowy. Zależy od: E02, E06.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Panel ustawień z jednym runnerem operacji i sekcjami, które nie niszczą danych poza „wyczyść bibliotekę”.

## Zakres

- Panel z nawigacją śledzącą przewijanie; runner operacji: jedna naraz, subskrypcja postępu przed wywołaniem, tylko własne fazy, sukces wyłącznie z wyniku komendy.
- Skan: dodaj folder (dialog systemowy), usuń root (potwierdzenie), auto-skan per root, skanuj ponownie, skanuj wszystko. Miniatury: renderuj wszystkie, ponów nieudane, zatrzymaj, wyczyść; status narzędzi wideo.
- Wygląd (jasny/ciemny), język (11), o programie (wersja, licencja, linki przez `open_url`).
- Strefa zagrożeń: wyczyść bibliotekę z potwierdzeniem, przez barierę mutacji tagów.

## Poza zakresem

- Import/eksport i duplikaty (E18).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/settings/SettingsPanel.tsx`
- `src/features/settings/SettingsNavigation.tsx`
- `src/features/settings/createSettings.ts`
- `src/features/settings/createSettingsOperationRunner.ts`
- `src/features/settings/progressService.ts`
- `src/features/settings/operationStateService.ts`
- `src/features/settings/statusMessage.ts`
- `src/features/settings/SectionOperationStatus.tsx`
- `src/features/settings/createScanSettings.ts`
- `src/features/settings/RemoveScanRootConfirmDialog.tsx`
- `src/features/settings/createDangerZoneSettings.ts`
- `src/features/settings/ClearLibraryConfirmDialog.tsx`
- `src/features/settings/sections`
- `src/app/AppSettingsView.tsx`
- `src/platform/tauri/dialogs.ts`

Testy do przeniesienia:
- `src/features/settings/__tests__`

Dokumenty i reguły:
- `docs/subsystems/settings-operations.md`
- `docs/subsystems/scanning-and-indexing.md`
- `.claude/rules/settings.md`
- `.claude/rules/scanning.md`

## Kryteria akceptacji

- Testy runnera, postępu i modeli skanu przeniesione.
- Druga operacja w trakcie pierwszej jest odrzucana synchronicznie.
- Testy „wyczyść bibliotekę” tylko na świeżych katalogach tymczasowych.
- Parytet `settings-scan`, `settings-appearance-language`, `settings-danger-about`.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E17 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `settings-scan`: Ustawienia, sekcja skanu z jednym rootem i statusem operacji.
- `settings-appearance-language`: Sekcje wyglądu i języka.
- `settings-danger-about`: Strefa zagrożeń i „o programie”.

## Ryzyka

- Dialogi systemowe (portal XDG) nie działają na prywatnym displayu: w scenariuszach wstrzykiwać ścieżkę.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
