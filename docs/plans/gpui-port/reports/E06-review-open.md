# E06: otwarte ustalenia przeglądu

Agent naprawczy luk (2026-11-21) po zakończonym workflow `gpui-port-epic`. Pętla przeglądu skończyła się czystą rundą (runda 5), więc ten plik nie zawiera otwartych ustaleń P0–P2 z przeglądu kodu. Zapisuje wynik sprawdzenia kryterium akceptacji „Parytet `shell-empty` i `shell-empty-light`” (różnica kroju pisma) oraz różnice wizualne, które zostały po ostatnim porównaniu.

## Zakres przebiegu

- Ścieżka startu GPUI i źródło rodziny fontu: src-gpui/crates/media_tagger_gpui/src/window.rs:282 (`init_application` rejestruje fonty przed pierwszym oknem, także na ścieżce `run_until_closed` i scenariuszy), src-gpui/crates/media_tagger_gpui/src/ui/fonts.rs:21 (`add_fonts` z Inter i JetBrains Mono), src-gpui/crates/media_tagger_gpui/src/ui/theme.rs:93 (korzeń okna dostaje `TYPOGRAPHY.sans[0]`), src-gpui/crates/media_tagger_gpui/src/views/shell.rs:163 (`app_root` na korzeniu powłoki).
- Kod GPUI przypiętego commitu: `gpui_linux` tworzy `CosmicTextSystem` dla X11 i Waylanda (to samo co w testach headless), `gpui_wgpu/src/cosmic_text_system.rs` (wybór twarzy, układ linii, rasteryzacja swash z hintingiem), `gpui/src/text_system.rs` (stos zastępczy).
- Porównanie `compare:ui shell-empty shell-empty-light` przez `job.py` (kod 0) i pomiary atramentu na obu parach PNG.

## Krój pisma (kryterium „Parytet shell-empty i shell-empty-light”)

| ID | Priorytet | Ustalenie | Stan |
| --- | --- | --- | --- |
| font-1 | P3 | Zgłoszony „inny, szerszy krój niż Inter” w oknie GPUI. Obie strony rysują Inter. Rodzina „Inter” rozwiązuje się w systemie tekstu GPUI do twarzy Inter (szerokość podtytułu 13 px w układzie GPUI 351,67 px, referencja HarfBuzz dla `public/fonts/Inter.ttf` 351,66 px, Noto Sans 352,80 px; zaawansowane kształtowanie nie zgłasza twarzy zastępczej). Różnica bierze się z rasteryzacji i zaokrągleń: GPUI układa z ułamkowymi przesunięciami (podtytuł 351 px atramentu), WebKitGTK pod Xvfb z przesunięciami zaokrąglonymi przez hinting (Inter 13 px: 360 px przesunięć, 358 px atramentu, tyle co zrzut Tauri). Swash z hintingiem rysuje glify wyższe i węższe (wysokość x 8 zamiast 7 wierszy, wznoszenia 11 zamiast 9), stąd wrażenie innego kroju. Według D10 (docs/plans/gpui-port/README.md:82, :186) rasteryzacja fontów jest poza parytetem. | nie jest wadą E06; bez zmiany kodu. Strażnik rejestracji na ścieżce startu: src-gpui/crates/media_tagger_gpui/tests/shell_window.rs:517 (bez `ui::fonts::register` w `init_application` pada: JetBrains Mono rozwiązuje się do „Adwaita Sans”) |
| font-2 | P2 | GPUI przypiętego commitu ładuje każdą twarz z wagą 400 i rasteryzuje bez osi wariacji, a za wagę twarzy zmiennej bierze jej domyślne 400. Dołączony `Inter.ttf` jest zmienny, więc bez Inter zainstalowanego w systemie każda waga (nagłówki 650, logotyp i przyciski 600) rysuje się jako 400 (pomiar: `No folders attached` 24 px ma 224,11 px dla wag 400, 600, 650 i 700, gdy fontconfig nie widzi systemowego Inter). Z systemowym Inter waga 650 bierze najbliższą statyczną twarz (tu Bold 700, 231,07 px). | nie naprawione: wymaga decyzji planu (dołączenie statycznych instancji Inter jako nowych zasobów albo zmiana GPUI, której przypięty commit nie ma); dotyczy wszystkich epików z tekstem, zapisane też w notatkach E22 |
| font-3 | P3 | Odstępy liter nie są stosowane: GPUI przypiętego commitu nie ma `letter_spacing` w stylu tekstu, a nagłówki mają `-0.025em` (src-gpui/crates/media_tagger_client/src/theme/tokens.rs:231), logotyp `-0.02em`, przyciski `-0.01em`. Razem z font-2 daje to nagłówek stanu pustego 229 px zamiast 223 px. | nie naprawione: ograniczenie GPUI; obejście (ręczne rozstawianie glifów) byłoby nową decyzją planu |

## Pozostałe różnice wizualne po ostatnim porównaniu

Te same w obu scenariuszach (`shell-empty` ciemny, `shell-empty-light` jasny); kolory tła, nagłówka, przycisków i stanu pustego zgodne z tokenami w obu motywach.

| Różnica | Pomiar | Właściciel |
| --- | --- | --- |
| Brak pola wyszukiwania z Ctrl+K, przyciskiem odświeżenia i Search w pierwszym wierszu nagłówka | puste miejsce między logotypem a przyciskiem ustawień | E12 |
| Brak segmentów rodzaju (All, Images, GIF, Video) i przycisku ulubionych | drugi wiersz zaczyna się pigułką liczby | E12 |
| Brak przycisku listy tagów z dzielnikiem | j.w. | E13 |
| Brak przełącznika Bulk actions z dzielnikiem | j.w. | E11/E14 |
| Pigułka liczby „0 items” stoi po lewej | x≈17 zamiast ≈343 (skutek trzech braków powyżej) | E12/E13/E14 |
| Stan pusty przesunięty w prawo | środek nagłówka 720 zamiast 713, przycisku 719,5 zamiast 712,5 (Tauri rezerwuje pas na pasek przewijania) | E09/E10 |
| Krój: ułamkowe przesunięcia i hinting swash | podtytuł 351 zamiast 358 px, linia bazowa 1 px wyżej, logotyp 1 px wyżej | poza parytetem (D10), font-1 |
| Nagłówek stanu pustego szerszy | 229 zamiast 223 px | font-2, font-3 (GPUI, decyzja planu) |

Pozycje i rozmiary przycisku ustawień, dzielnika, przycisków okna, suwaka „Tile size”, kafelka ikony i przycisku „Add first folder” są zgodne (różnica do 1 px). Zgłoszony wcześniej brak zmiany koloru ikony Back po najechaniu dotyczy strony ustawień w stanie najechania, którego te scenariusze nie pokazują; nie oceniono go tutaj.

## Co zostaje otwarte

- font-2 (P2) i font-3 (P3): czekają na decyzję planu; żadne z nich nie blokuje E06 i żadne nie jest P0 ani P1.
- Różnice należące do E09–E14 zamkną ich epiki.
- Ustaleń E06 do naprawy w kodzie epiku: brak.
