# E19: Domknięcie: parytet, wydajność, klawiatura, dokumentacja

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E10, E14, E16, E18.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Potwierdzić, że aplikacja GPUI robi to samo co Tauri (poza D8), i zostawić po sobie dokumentację oraz narzędzia przeglądu.

## Zakres

- Pełny przebieg wszystkich scenariuszy porównania; lista różnic z decyzją „naprawić / świadoma różnica”.
- Przegląd klawiatury i fokusu względem `docs/subsystems/lightbox.md` i konwencji frontendu.
- Wydajność na syntetycznej bazie 1 000 000 pozycji: przewijanie, odświeżenie zapytania, pamięć po długiej sesji.
- Dokumentacja: nowa strona `docs/frontend/gpui-architecture.md`, aktualizacja `system-overview.md`, `testing.md`, `CLAUDE.md`, reguł `.claude/rules/`.
- ADR 0036 → Accepted; wpisy `HOT_PATHS` dla plików siatki i miniatur sprawdzone na istniejących ścieżkach.
- Batche `/audit` (`.claude/skills/audit/references/batches.json`) i jego test pokrycia obejmują źródła w `src-gpui/`.
- Lista braków → epiki odroczone (E20–E22) z opisem stanu.

## Poza zakresem

- Usunięcie Tauri i identyfikator wydania (E22).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `docs/plans/gpui-port/README.md`
- `e2e/gallery-scale`
- `src-tauri/tests/grid_baseline.rs`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`
- `docs/development/testing.md`
- `docs/adr/README.md`

## Kryteria akceptacji

- Każdy scenariusz ma werdykt; brak różnic „blocking”.
- Pomiar wydajności zapisany w notatkach z porównaniem do Tauri na tym samym seedzie.
- `bun run test:all` zielone; dokumentacja zgodna ze źródłem.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E19 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `python3 -m unittest discover -s .claude/skills/review-changes/scripts -p test_collect_scope.py`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- brak

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
