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

done 2026-11-21

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-20 — implementer E00

- Z epiku E00: klient ma minimalne `i18n::Catalog` (tylko `en.json`, zwykłe klucze z kropkami, bez liczby mnogiej i interpolacji) i `theme::css_color_token` (tylko kolory hex z bloków `@plugin "daisyui/theme"` w `src/styles.css`; tokeny pochodne z `var()`/`color-mix()` nie są rozwiązywane). Oba pliki są wczytywane przez `include_str!("../../../../src/…")`. E03 je zastępuje albo rozszerza; `hello.rs` korzysta z obu.

### 2026-11-21 — workflow gpui-port-epic

- Zrobione: i18n w `media_tagger_client::i18n` (`i18n.rs` i `i18n/`): wszystkie 11 plików locale wbudowane przez `include_str!`, klucze z kropkami, fallback do angielskiego, a dla klucza bez tłumaczenia sam klucz (jak i18next). Liczba mnoga w kolejności i18next (`_zero`, `_<kategoria>`, klucz bazowy), ręczne reguły CLDR dla 11 języków przypięte testami do `Intl.PluralRules` z Node 22 (liczby 0–120, wektor 0, 1, 2, 5, 12, 21, 22, 101, miliony, ułamki, ujemne). Interpolacja jak w i18next, `format_count` odtwarza `Intl.NumberFormat`, `translate_backend_text` i `Params::from_json` odpowiadają `backendText.ts`. Język startowy: zapisana preferencja, potem `LC_ALL`, `LC_MESSAGES`, `LANG`, potem angielski. Motyw: tokeny obu motywów jako stałe Rust (kolory, promienie, cienie, ruch razem z czasami modala lightboxa, skala typograficzna); czytnik CSS tylko w testach porównuje je z `src/styles.css`, a testy dowodzą, że parytet pada przy zmianie wartości po jednej stronie lub nowym tokenie w CSS. Preferencje: port `PrefsStore` (`load` nigdy nie zawodzi, `save`), `FilePrefsStore` zapisuje atomowo w katalogu profilu, `MemoryPrefsStore` za feature `test-support`. Testy: 15 nowych (`tests/i18n_contract.rs`, `tests/prefs_contract.rs`), w tym port 5 przypadków `theme.test.ts` i zachowań zapisanego języka z `core.ts`. Zaktualizowane dokumenty: localization.md, architecture-and-ui-conventions.md, testing.md, system-overview.md.
- Odkryte: `theme::Rgb` zastąpiony przez `theme::Rgba { r, g, b, a: u8 }` (`to_rgba_u32()` z `gpui::rgba`); usunięto `css_color_token` (runtime CSS). `ThemeName::tokens()` zwraca `&ThemeTokens`; stałe `LIGHT`, `DARK`, `RADII`, `MOTION`, `TYPOGRAPHY`. `text-xs`, `text-xl`, interlinie i grubości fontów to domyślne wartości Tailwinda (test czyta `node_modules/tailwindcss/theme.css`, pomija się bez instalacji). Celowo niesione jako stałe nie są (`NOT_CARRIED` w `theme/parity_tests.rs`): aliasy oklch `--p`, `--pc`, `--s`, `--bc`, `--b1`, `--b2`, `--b3`, `--su` (most natywnego odtwarzacza, E21), `--select-chevron` (ikona, E05), `--depth`/`--noise` (test sprawdza 0). Kolory komponentów na sztywno (powierzchnia wideo lightboxa `#0b0b0b`, `.theme-preview--*`) i czasy zanikania miniatur zostają epikom E16, E17, E09. API i18n: `I18n::new`/`with_locales`, `t`, `t_with`, `resolve`, `format_count`, `backend_text`; `Params::new().with(..).with_count(n)`, `formattedCount` trzeba podać jawnie. Klucz wskazujący grupę zachowuje się jak brakujący (i18next pokazałby komunikat o obiekcie). Pusty przekład jest zwracany bez zmian, test pilnuje, że żaden locale nie ma pustego liścia. Nieobsługiwane, bo żaden zasób tego nie używa: zagnieżdżone `$t()`, konteksty, liczebniki porządkowe, przestrzenie nazw, funkcje formatu. Precedencja języka systemu to `LC_ALL` > `LC_MESSAGES` > `LANG` (pierwsza ustawiona i niepusta decyduje, nieobsługiwana wartość jak `C` daje angielski). Preferencje: `<katalog profilu>/ui-preferences.json` z polami `theme` i `language`, pliki tymczasowe `.mediatagger-ui-preferences-<pid>-<n>.pending`; parsowanie jak localStorage, każde pole osobno (motyw dokładnie `light`/`dark`, język z normalizacją prefiksu); domyślnie `Prefs::theme_or_default()` (dark) i `Prefs::language_or_system`. Plik preferencji z BOM UTF-8 jest uznawany za uszkodzony i daje wartości domyślne (test to zakłada, aplikacja BOM nie zapisuje). `PrefsStore` robi I/O: adapter ma wołać `load` przy starcie, a `save` na executorze, nigdy na wątku UI; nieudany zapis nie blokuje zmiany motywu ani języka. Dodanie języka wymaga teraz też wariantu `Language`, `locale_source`, reguły liczby mnogiej i grupowania cyfr w crate klienta. Odroczone: rejestracja fontów i komponenty używające tokenów typografii i cieni (E05); wczytanie preferencji i języka systemu do działającej aplikacji GPUI (E06), zmiana ich z ustawień (E17); formatowanie liczb obejmuje tylko całkowite (`i64`), bez ułamków i notacji wykładniczej z JSON backendu. Pominięte przypadki testów TS: `theme.test.ts` atrybut `data-theme` na dokumencie (mechanika DOM, brak modelu apply-theme w E03), `syncNativeWindowTheme` (most Tauri, GPUI nie ma osobnego motywu okna), `createVirtualList.test.ts` (należy do E13), `persistLanguage` przy starcie i zdarzeniu `languageChanged` (okablowanie Solid/i18next i atrybut `lang`; zapis wybranego języka pokrywa test na poziomie portu). Różnice wizualne parity: brak (0 rund, brak otwartych).
- Zostało: nic zablokowanego, brak niepowodzeń. Przegląd: 2 rundy, czysty, 2 potwierdzone i naprawione, 0 odrzuconych, brak otwartych, spornych, pre-existing, niepewnych ani luk (raport `E03-review-open.md` niepotrzebny). Brama: ok po 1 rundzie. Dla późniejszych epików: E05, E06, E09, E16, E17, E21 jak wyżej.
- Testy:
  - `cargo clippy ... -D warnings` na src-gpui: pass
  - `cargo fmt --check` na src-gpui: pass
  - `cargo test -p media_tagger_client` (209 unit + 2 + 11 + 15 nowych integracyjnych): pass
  - `cargo test -p media_tagger_gpui`: pass
  - `architecture_boundaries` (33): pass
  - `bun run locale:check`: pass
  - `bun run gpui:lock-parity`: pass
  - checks (fixes 0, failures brak): pass
  - `bun run test:all` (brama, 1 runda): pass
