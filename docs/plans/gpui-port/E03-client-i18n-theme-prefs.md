# E03: i18n, tokeny motywu i preferencje UI

Rozmiar: M. Poziom: standardowy. Zależy od: E00.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Te same teksty, kolory i preferencje co w aplikacji Tauri, z jednego źródła.

## Zakres

- Silnik i18n zgodny z używanym podzbiorem i18next: 11 plików `src/i18n/locales/*.json` przez `include_str!`, klucze z kropkami, `{{placeholder}}` bez escapowania, liczba mnoga CLDR (`_one/_few/_many/_other`) dla en, pl, fr, de, it, es, ru, zh, ja, ko, cs, fallback na en, formatowanie liczb dla `formattedCount`.
- Odpowiednik `backendText.ts`: `i18n:<klucz>`, JSON `{"key","params"}` i goły klucz z kropkami.
- Wybór języka: preferencje → `LC_MESSAGES`/`LANG` → en; etykiety z `languages.ts`.
- Tokeny motywu `light`/`dark` jako stałe Rust (kolory, promienie, skala typografii, czasy animacji, cienie) + test parytetu parsujący `src/styles.css`.
- Port `PrefsStore` + adapter plikowy (zapis atomowy w katalogu profilu) dla motywu i języka (D6).

## Poza zakresem

- Rejestracja fontów i komponenty (E05).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/i18n`
- `src/styles.css`
- `src/ui/dom/theme.ts`
- `scripts/validate-locales.mjs`

Testy do przeniesienia:
- `src/i18n/__tests__`
- `src/ui/dom/__tests__`

Dokumenty i reguły:
- `docs/development/localization.md`
- `.claude/rules/localization.md`

## Kryteria akceptacji

- Wektory liczby mnogiej (0, 1, 2, 5, 12, 21, 22, 101) dla wszystkich 11 języków.
- Każdy klucz z `en.json` rozwiązuje się w każdym języku; brak klucza → en, nigdy pusty tekst.
- Test parytetu tokenów ↔ `src/styles.css` pada po zmianie koloru tylko po jednej stronie.
- Preferencje: roundtrip, uszkodzony plik → wartości domyślne bez paniki.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E03 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `bun run locale:check`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Reguły CLDR pisane ręcznie: pokryć tabelą wektorów, nie pamięcią.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: klient ma minimalne `i18n::Catalog` (tylko `en.json`, zwykłe klucze z kropkami, bez liczby mnogiej i interpolacji) i `theme::css_color_token` (tylko kolory hex z bloków `@plugin "daisyui/theme"` w `src/styles.css`; tokeny pochodne z `var()`/`color-mix()` nie są rozwiązywane). Oba pliki są wczytywane przez `include_str!("../../../../src/…")`. E03 je zastępuje albo rozszerza; `hello.rs` korzysta z obu.
