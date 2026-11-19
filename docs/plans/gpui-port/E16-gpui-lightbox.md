# E16: Lightbox: UI, przeglądarka obrazów, zaślepki GIF i wideo

Rozmiar: L. Poziom: trudny (model najwyższy dla implementacji i poprawek). Zależy od: E09, E13, E15.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Pełny lightbox dla obrazów; GIF i wideo jako zaślepki z działającym panelem bocznym (D8).

## Zakres

- Modal, scena mediów, pasek narzędzi, szyna akcji (ulubione, reset zoomu, pełny ekran, usuń), panele tagów, informacji i grupy mediów; zwijanie panelu w wąskim oknie; wygaszanie kontrolek.
- Przeglądarka obrazów: dekodowanie poza wątkiem UI, zoom 1–12×, kółko zakotwiczone na wskaźniku, dwuklik, przeciąganie z ograniczeniem, klawisze ←/→, `f`, `0`, `+`/`=`/`-`; pełny ekran przez okno.
- Potwierdzenie usunięcia z wpisanym słowem (`lightbox.deleteConfirm.confirmWord` nigdy nie jest tłumaczone maszynowo).
- GIF: statyczna pierwsza klatka + chip „GIF”. Wideo: panel-zaślepka z miniaturą jako plakatem, etykietą „Video” i metadanymi; bez odtwarzania. Nowe klucze i18n zaślepek we wszystkich 11 plikach.

## Poza zakresem

- Animacja GIF (E20).
- Natywne wideo przez libmpv (E21).

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/lightbox`
- `src/app/LightboxBoundary.tsx`

Testy do przeniesienia:
- `src/features/lightbox/__tests__`

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`
- `.claude/rules/lightbox-video.md`

## Kryteria akceptacji

- Testy pan/zoom (`lightboxPanZoomService`, `createImageControls`) przeniesione.
- Klawisze i fokus jak w Solid; Escape zamyka tylko górną warstwę.
- Parytet `lightbox-image`, `lightbox-image-zoomed`, `lightbox-delete-confirm`; `lightbox-gif` i `lightbox-video` porównywane poza sceną mediów.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E16 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`
- `bun run locale:check`

## Scenariusze porównania

- `lightbox-image`: Lightbox z obrazem, panel boczny otwarty.
- `lightbox-image-zoomed`: Ten sam obraz po zoomie i przesunięciu.
- `lightbox-gif`: Lightbox z GIF-em (GPUI: zaślepka, D8).
- `lightbox-video`: Lightbox z wideo (GPUI: zaślepka, D8).
- `lightbox-delete-confirm`: Dialog potwierdzenia usunięcia z wpisanym słowem.

## Ryzyka

- Bardzo duże obrazy: limit pamięci dekodowania.

## Status

todo

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).
