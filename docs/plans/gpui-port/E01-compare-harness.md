# E01: Harness porównawczy Tauri ↔ GPUI

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E00.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Jedna komenda, która na tych samych syntetycznych danych robi zrzut ekranu tego samego stanu w obu aplikacjach, żeby agenci mogli porównywać efekty.

## Zakres

- Build E2E GPUI do `src-gpui/target-e2e` ze strażnikami jak w `e2e/build-e2e.js` (identyfikator `.gpui.e2e`, tytuł „Tagrove GPUI E2E”, target dir).
- `bun run gpui:control start|scenario|screenshot|stop`: prywatny display i katalogi XDG jak w `e2e/control.js` albo ścieżka headless z E00; własny lock `artifacts/gpui-control/active.lock`; zawsze `stop`, także po błędzie.
- Scenariusze: start w nazwanym, deterministycznym stanie (`--scenario <id>` działa tylko w profilu `.gpui.e2e`), seed z `e2e/fixtures.js` (`copyPngFixtures`, `createPlayableFixtures`) w świeżym katalogu tymczasowym.
- `bun run compare:ui <scenario…>`: ten sam seed → Tauri przez `app:control` (kroki w `e2e/compare/scenarios/*.js`) i GPUI → `artifacts/gpui-compare/<scenario>/{tauri,gpui,side-by-side}.png` + `result.json`.
- Pierwsze scenariusze: `shell-empty`, `shell-empty-light`. Katalog scenariuszy żyje w README planu; każdy epik UI dopisuje swoje po obu stronach.
- Testy `node --test` strażników i katalogu scenariuszy; sekcja w `docs/development/testing.md`.

## Poza zakresem

- Porównanie piksel w piksel i progi różnic.
- Sterowanie GPUI przez WebDriver.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `e2e/control.js`
- `e2e/control-support.js`
- `e2e/control-native.py`
- `e2e/display.js`
- `e2e/build-e2e.js`
- `e2e/fixtures.js`
- `.claude/skills/verify-mediatagger/SKILL.md`

Testy do przeniesienia:
- `e2e/build-e2e.test.js`
- `e2e/display.test.js`
- `e2e/control-support.test.js`

Dokumenty i reguły:
- `docs/development/testing.md`

## Kryteria akceptacji

- `bun run compare:ui shell-empty` zostawia trzy PNG i `result.json`; ponowne uruchomienie daje ten sam układ.
- Sesja nie startuje, gdy istnieje `artifacts/app-control/active.lock` albo trwa `test:e2e:tauri`; nigdy nie usuwa cudzego locka ani nie zabija procesów po nazwie.
- Strażniki odmawiają złego identyfikatora, tytułu i target dir (testy).
- Po przerwaniu nie zostaje proces, display ani katalog tymczasowy.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E01 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `node --test e2e/compare`

## Scenariusze porównania

- `shell-empty`: Pusta biblioteka, widok galerii (pusty stan), motyw ciemny.

## Ryzyka

- Wspólne zasoby z desktop E2E (`dist/`, `target-e2e`, porty 4444/4446/4447): tylko seryjnie.
- Zrzuty i logi zostają w ignorowanym `artifacts/`; nic z nich nie trafia do śledzonych plików.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
