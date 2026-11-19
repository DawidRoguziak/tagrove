# E22: Przełączenie: identyfikator wydania, pakowanie, usunięcie Tauri

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E19, E21.

**Odroczony.** Nie uruchamiać workflow bez nowej decyzji użytkownika; przed startem powstaje osobny plan tego epiku.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zastąpić aplikację Tauri aplikacją GPUI.

## Zakres

- Osobny plan i ADR: identyfikator wydania i migracja danych profilu, pakowanie (Flatpak/AppImage/native), usunięcie frontendu Solid i adaptera Tauri, E2E dla GPUI.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `packaging`
- `src-tauri/tauri.conf.json`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/development/linux-packaging.md`

## Kryteria akceptacji

- Do ustalenia w planie epiku.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E22 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Migracja danych użytkownika.

## Status

deferred

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
