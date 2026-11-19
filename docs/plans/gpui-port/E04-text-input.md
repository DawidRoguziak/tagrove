# E04: Pole tekstowe (własny element GPUI)

Rozmiar: M. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E03.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Jednowierszowe pole tekstowe, którego GPUI nie ma w zestawie, zbudowane u nas zamiast brać bibliotekę komponentów od osoby trzeciej (D1).

## Zakres

- Model edycji w kliencie (czysty, bez GPUI): bufor, kursor, zaznaczenie, ruch po grafemach i słowach, wstawianie, usuwanie, zaznacz wszystko, stos cofnij/ponów.
- Element GPUI: rysowanie tekstu, kursora i zaznaczenia, przewijanie w poziomie przy przepełnieniu, placeholder, stan wyłączony, obwódka fokusu z tokenów motywu.
- Wejście: klawiatura (strzałki, Home/End, z Shift i Ctrl), mysz (klik, przeciąganie, dwuklik na słowie), schowek (kopiuj, wytnij, wklej), kompozycja IME przez handler wejścia GPUI.
- Kontekst klawiszy: skróty globalne (np. Ctrl+K, klawisze lightboxa) nie odpalają się, gdy pole ma fokus i samo obsługuje klawisz; Enter i Escape trafiają do właściciela pola.
- Punktem wyjścia jest przykład `crates/gpui/examples/input.rs` z przypiętego commita Zed (Apache-2.0): zachować informację o pochodzeniu w nagłówku pliku.
- Miejsca użycia w kolejnych epikach: wyszukiwarka, edytor tagów, klucz grupy, słowo potwierdzenia usunięcia, ścieżki mapowania rootów.

## Poza zakresem

- Pole wielowierszowe, formatowanie i podświetlanie składni.
- Chipy i podpowiedzi (E12, E13).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/search/SearchTagator.tsx`
- `src/features/search/suggestions/inputService.ts`
- `src/features/tagging/TagEditor.tsx`
- `src/features/lightbox/LightboxDeleteConfirmDialog.tsx`
- `src/features/lightbox/MediaGroupSetter.tsx`

Testy do przeniesienia:
- `src/features/search/suggestions/__tests__`
- `src/features/tagging/__tests__`

Dokumenty i reguły:
- `docs/frontend/architecture-and-ui-conventions.md`

## Kryteria akceptacji

- Testy modelu edycji: emoji i sekwencje ZWJ, znaki łączące, CJK, ruch po słowach, zaznaczanie w obie strony, cofnij/ponów, wklejenie tekstu z nową linią (zamiana na spację albo odrzucenie, opisane).
- Testy `#[gpui::test]`: fokus, wpisywanie, skróty schowka, Enter/Escape do właściciela, skrót globalny nie odpala się podczas pisania.
- Kompozycja IME sprawdzona ręcznie albo testem na handlerze wejścia; wynik w notatkach.
- Długi tekst przewija się, a kursor zostaje widoczny.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- unicode-segmentation

## Testy i sprawdzenia

Uruchamiane przez `job.py start E04 checks`:
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

- IME i kompozycja różnią się między X11 i Waylandem.
- Indeksy bajtowe UTF-8 kontra UTF-16 w API wejścia GPUI: błąd daje panikę na granicy znaku.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
