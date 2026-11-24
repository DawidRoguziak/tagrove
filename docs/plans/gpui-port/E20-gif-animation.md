# E20: Animacja GIF w lightboxie

Rozmiar: M. Poziom: standardowy. Zależy od: E16.

**Odroczony.** Nie uruchamiać workflow bez nowej decyzji użytkownika; przed startem powstaje osobny plan tego epiku.

Przed startem przeczytaj [README planu](README.md) (Stan docelowy, Decyzje, Zasady dla agentów), [NOTES.md](NOTES.md) i notatki epików, od których ten zależy.

## Cel

Zastąpić zaślepkę GIF animacją.

## Zakres

- Sprawdzić natywną animację elementu obrazu GPUI; limit pamięci klatek; pauza poza widokiem.

## Poza zakresem

- brak

## Źródła zachowania (specyfikacja)

Implementacja Solid/Tauri i jej testy są specyfikacją zachowania; dokumenty są kontraktem.

Kod:
- `src/features/lightbox/LightboxMediaStage.tsx`

Testy do przeniesienia:
- brak

Dokumenty i reguły:
- `docs/subsystems/lightbox.md`

## Kryteria akceptacji

- GIF animuje się w lightboxie; pamięć ograniczona dla długich animacji.
- `bun run test:all` zielone; aplikacja Tauri bez zmian zachowania.

## Dozwolone nowe zależności

- brak (każda inna zależność: zatrzymaj się i zgłoś `blocked`)

## Testy i sprawdzenia

Uruchamiane przez `job.py start E20 checks`:
- `bun run rust:fmt`
- `bun run rust:clippy`
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --test architecture_boundaries`
- `cargo fmt --manifest-path src-gpui/Cargo.toml --all -- --check`
- `cargo clippy --manifest-path src-gpui/Cargo.toml --all-targets --all-features --locked -- -D warnings`
- `bun run gpui:lock-parity`
- `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_gpui`

## Scenariusze porównania

- `lightbox-gif`: Lightbox z GIF-em (GPUI: zaślepka, D8).

## Ryzyka

- brak

## Status

deferred

## Notatki agentów

Wpisy tylko dopisywane, format w [README](README.md#protokół-notatek).

### 2026-11-22 — implementer E16

- Z epiku E16: lightbox pokazuje GIF jako pierwszą klatkę: `media_tagger_client::lightbox::gif_first_frame` obcina plik do nagłówka, globalnej palety, pierwszego obrazu i trailera, a `media_tagger_gpui::media::Originals` dekoduje to przez `gpui::Image::to_image_data` (ADR 0046). Animacja wymaga zastąpienia tego kroku ograniczonym buforem klatek (budżet `decode_refusal(wymiary, liczba_klatek)` już liczy klatki) i rysowania klatki o indeksie innym niż 0 w `views/lightbox/stage.rs` (`window.paint_image(..., frame_index, ...)`). Chip „GIF” na scenie (`GIF_CHIP_SELECTOR`, opis `lightbox.placeholder.gifStill`) jest zaślepką D8: po dodaniu animacji usuń chip i klucz albo zostaw chip bez opisu. Animowany WebP jest dziś dekodowany ze wszystkimi klatkami i rysowana jest pierwsza.

### 2026-11-24 — implementer E19

- Z epiku E19 (stan przy domknięciu portu): GIF w lightboxie to nadal pierwsza klatka z chipem „GIF” (D8). W pełnym przebiegu porównania scenariusz `lightbox-gif` różni się od Tauri tylko sceną: chip w lewym górnym rogu i brak animacji; panel boczny, szyna akcji i układ są zgodne. Kafel GIF-a w siatce jest w obu aplikacjach statyczną miniaturą z chipem, więc E20 dotyczy wyłącznie sceny lightboxa.
- Z epiku E19: mały obraz powiększony wielokrotnie (GIF 96×64 na scenie 1058 px) ma w GPUI krawędzie wygaszane na szerokości połowy piksela źródła (ok. 5 px przy 11-krotnym powiększeniu), bo sampler GPU miesza skrajne teksele z przezroczystością; WebKit rysuje ostrą krawędź. Dotyczy też małych PNG. Zapisane jako świadoma różnica (`docs/frontend/gpui-architecture.md`, „Parity with the Tauri interface”); przy animacji klatek warto sprawdzić, czy da się to obejść marginesem powielonych pikseli.
- Z epiku E19: pomiar pamięci na milionie pozycji (`tests/million.rs`) nie obejmuje lightboxa ani GIF-ów. Budżet bufora klatek animacji trzeba zmierzyć osobno; pamięć podręczna miniatur trzyma 256 MiB zdekodowanych obrazów i nie zostawia zapasu na klatki.
