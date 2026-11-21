# E05: Prymitywy UI i warstwa nakładek (GPUI)

Rozmiar: L. Poziom: standardowy. Zależy od: E03, E04.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zestaw kontrolek i jedna warstwa modali, z których składają się wszystkie ekrany.

## Zakres

- Motyw jako global GPUI z tokenów klienta; rejestracja fontów Inter i JetBrains Mono z `public/fonts`.
- 22 ikony z `UiIcon.tsx` jako zasoby SVG.
- Kontrolki: przycisk (warianty), przycisk-ikona, chip, alert, pasek postępu, suwak, kontrolka segmentowa z przesuwanym podświetleniem, przełącznik, spinner z opóźnieniem, pole tekstowe z E04 ostylowane tokenami.
- Stos warstw i modal: Escape tylko dla górnej warstwy, pułapka fokusu i przywrócenie fokusu, wejście/wyjście wyłącznie przez opacity, modal zostaje zamontowany podczas wygaszania.
- Scenariusz `kit` (tylko profil `.gpui.e2e`): plansza wszystkich kontrolek w obu motywach.

## Poza zakresem

- Ekrany funkcji.

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/ui/controls`
- `src/ui/overlays`
- `src/ui/dom`
- `src/styles.css`

Testy do przeniesienia:
- `src/ui/controls/__tests__`
- `src/ui/overlays/__tests__`
- `src/ui/dom/__tests__`

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`
- `.claude/rules/frontend.md`

## Kryteria akceptacji

- Testy `#[gpui::test]`: kolejność Escape w stosie warstw, pułapka i przywrócenie fokusu, klawiatura suwaka i kontrolki segmentowej.
- Zrzut `kit` w obu motywach zgodny z tokenami (ocena agenta parytetu względem `src/styles.css`).
- Żadnego tekstu na sztywno; etykiety dostępności z i18n.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E05 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- brak (epik bez widocznego odpowiednika w Tauri albo bez UI)

## Ryzyka

- Nie animować wymiarów ani layoutu (konwencja frontendu).

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-21 — implementer E04

- Z epiku E04: pole tekstowe to `media_tagger_gpui::ui::text_input::TextInput` (encja; model edycji w `media_tagger_client::text`, ADR 0038). Jest już ostylowane tokenami `input` z `src/styles.css` przez `text::field_visuals` (tło, obwódka i pierścień fokusu jako cienie, placeholder, zaznaczenie, przezroczystość 0,55 dla wyłączonego); `FieldChrome::Bare` zdejmuje ramkę (pole w edytorze tagów), `set_height` daje stałą wysokość (`h-8`, `h-10`). Motyw dostaje dziś w konstruktorze (`TextInput::new(theme, window, cx)`) i przez `set_theme`: global motywu z tego epiku powinien to zastąpić albo wołać `set_theme` przy zmianie.
- Z epiku E04: pole dziedziczy font, rozmiar i interlinię ze stylu tekstu rodzica (`window.text_style()`); bez rejestracji fontów rysuje fontem systemowym. Kursor nie mruga (stabilne zrzuty).
- Z epiku E04: `ui::init(cx)` rejestruje skróty klocków z `ui/` przy starcie (`window::run_until_closed`); nowe kontrolki dopisują tam swoje. Zasady skrótów z ADR 0038 obowiązują każdy skrót: nigdy bez kontekstu klawiszy, a skrót na klawiszu wpisującym znak (litera, cyfra, spacja, także z Shift) z kontekstem `text_input::outside_text_input(kontekst)`. Skrót bez kontekstu zarejestrowany po `ui::init` odebrałby polu Enter, Escape i Ctrl+A.
- Z epiku E04: Escape modala: pole wysyła `text_input::actions::Cancel` (i `Confirm` dla Enter), których samo nie obsługuje. Nieobsłużony przez właściciela Escape spada do następnego powiązania klawisza `escape` w płytszym kontekście, więc Escape górnej warstwy wystarczy powiązać w kontekście modala. Tab nie jest powiązany w polu; uchwyt fokusu pola jest przystankiem tabulacji (`tab_stop(true)`), wyłączone pole nim nie jest.
