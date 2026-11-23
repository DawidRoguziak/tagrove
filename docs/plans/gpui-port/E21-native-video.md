# E21: Natywne wideo (libmpv) w oknie GPUI

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E16.

**Odroczony.** Nie uruchamiać workflow bez nowej decyzji użytkownika; przed startem powstaje osobny plan tego epiku.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Odtwarzanie wideo w lightboxie GPUI.

## Zakres

- Osobny plan przed startem: API renderowania libmpv do tekstury albo powierzchni GPUI, sterowanie, zdarzenia, pełny ekran; ponowne użycie `src-tauri/src/video/mpv_events.rs` i `types.rs`.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src-tauri/src/video`
- `src/features/lightbox/video`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`
- `docs/adr/0012-non-blocking-native-video-render.md`

## Kryteria akceptacji

- Do ustalenia w planie epiku.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E21 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `lightbox-video`: Lightbox z wideo (GPUI: zaślepka, D8).

## Ryzyka

- Wymaga decyzji architektonicznej i ADR.

## Status

deferred

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-22 — implementer E16

- Z epiku E16: wideo w lightboxie to zaślepka w `views/lightbox/stage.rs` (`StageContent::Video`): miniatura jako plakat (`Thumbnails::sync_poster`), chip „Video”, linie metadanych (`lightbox::video_placeholder_lines`) i `lightbox.placeholder.videoUnavailable`. `Originals` niczego dla wideo nie czyta. Miejsca przygotowane pod odtwarzacz: `Session::toggle_fullscreen` i klawisz `f` dla wideo są przyjmowane i nic nie robią (gałąź `kind.is_video()`), `Session::sync_fullscreen` wychodzi z pełnego ekranu obrazu po wybraniu wideo, kolumna mediów ma już rezerwę 44 px nad sceną (`layout::video::TOP_RESERVE`) i powierzchnię `#0b0b0b`, a komunikat o błędzie szczegółów stoi 88 px nad dołem sceny. Nie ma jeszcze: `nativePanelReady` (panel boczny czeka na potwierdzenie granic natywnej powierzchni), ukrywania kolumny mediów w wąskim oknie z otwartą szufladą, pełnego ekranu wideo ukrywającego panele ani klawiszy odtwarzacza. Po dodaniu odtwarzania usuń klucz `lightbox.placeholder.videoUnavailable` z 11 plików.
