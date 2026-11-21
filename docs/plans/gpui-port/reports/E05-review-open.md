# E05: otwarte ustalenia przeglądu

Zastępczy przebieg soczewki `review-gpui` (agent naprawczy luk, 2026-11-21). Zamyka lukę rund 1 i 2 pętli przeglądu workflow, w których agent `review-gpui` nic nie zwrócił.

## Zakres przebiegu

Cała zmiana E05 względem 1179cbb, czytana z kodem GPUI przypiętego commitu (`window.rs`, `app.rs`, `elements/div.rs`, `elements/animation.rs`, `tab_stop.rs`, shader `fs_shadow` w `gpui_wgpu/src/shaders.wgsl`):

- warstwy: adapter → klient → rdzeń; `controls`, `overlay`, `icons`, `kit` w kliencie bez GUI i bez rdzenia;
- blokowanie wątku UI (brak wywołań portu, odczytów plików ani dekodowania w nowych widokach; fonty i ikony wkompilowane statycznie);
- czasy życia encji, subskrypcji i wywołań zwrotnych: `on_next_frame` w `ui/layers.rs` (sprawdza global i id warstwy), timer zanikania modala (`Task` w `ModalState`, token generacji w `ModalPresence::finish`), `on_release` modala (`layers::forget`), opóźniony spinner (`with_animation`, bez timerów), słuchacze okna przy przeciąganiu suwaka (rejestrowane co klatkę);
- fokus i klawisze: kontekst `UiLayers`, kolejność Escape, Tab i Shift+Tab, pułapka i przywracanie fokusu, `focus_root` i `on_focus_lost`, kontekst pola `TextInput` z E04 i zasady ADR 0038 (skróty suwaka i segmentów tylko w ich kontekstach, bez klawiszy wpisujących znak);
- poprawność `notify` i przerysowania kontrolowanych `Slider` i `SegmentedControl`;
- parytet i18n (brak tekstów na sztywno w `ui/` i `views/kit.rs`) i motywu ze źródłami Solid w `src/ui/controls`, `src/ui/overlays`, `src/ui/dom` i `src/styles.css`.

## Potwierdzone ustalenia

| ID | Priorytet | Ustalenie | Stan |
| --- | --- | --- | --- |
| gpui-1 | P2 | Podświetlenie kontrolki segmentowej mierzone jest w prepaint (`on_children_prepainted`), a `cx.notify()` w fazie rysowania nie oznacza okna jako brudnego (`WindowInvalidator::invalidate_view` poza `DrawPhase::None`). Bez innego zdarzenia podświetlenie nie pojawiało się po pierwszym pokazaniu grupy i zostawało na starym segmencie po wyborze strzałką, a nowy segment miał tekst `primary-content` bez tła. | naprawione: src-gpui/crates/media_tagger_gpui/src/ui/segmented.rs:241 (przerysowanie w następnej klatce); test src-gpui/crates/media_tagger_gpui/tests/controls.rs:737 (bez poprawki pada w obu częściach) |
| gpui-2 | P3 | Zamykany modal zostawiał fokus na swoim przycisku, gdy przywrócenie nie miało celu (modal otwarty przy fokusie na korzeniu): przez 100 ms zanikania Enter i Space aktywowały przycisk zamykanego dialogu, wbrew `inert` w `UiModal.tsx` i wbrew opisowi „takes no input”. | naprawione: src-gpui/crates/media_tagger_gpui/src/ui/modal.rs:151 (fokus z zamykanej treści na korzeń); test src-gpui/crates/media_tagger_gpui/tests/overlays.rs:310 (bez poprawki Enter aktywuje przycisk) |
| gpui-3 | P3 | Parytet motywu: GPUI rysuje zewnętrzny cień pod całym elementem (`fs_shadow` bez otworu), CSS tylko poza ramką. Przycisk z półprzezroczystym tłem pokazywał `--shadow-control` w środku: w jasnym motywie aktywny przycisk ikonowy był o około 17 poziomów ciemniejszy (#C7D7D2 zamiast #D8E8E3), wyłączony przycisk główny o 5. | naprawione: src-gpui/crates/media_tagger_gpui/src/ui/theme.rs:75 i src-gpui/crates/media_tagger_gpui/src/ui/button.rs:192 (przezroczysty element zachowuje tylko cienie wewnętrzne); testy src-gpui/crates/media_tagger_gpui/tests/kit.rs:175 (piksele, bez poprawki pada) i src-gpui/crates/media_tagger_gpui/src/ui/theme.rs:127 |

Otwartych ustaleń P0–P2: brak. P0 ani P1 nie znaleziono.

## Odrzucone, niepewne, zastane

- Odrzucone: wyciek encji przez `entity.clone()` w słuchaczach przeciągania suwaka (słuchacze okna są czyszczone co klatkę); zgubione `notify` po timerze modala (encja `ModalState` jest czytana przy renderze właściciela i śledzona przez okno); `request_animation_frame` w `Modal::render` (`on_next_frame` działa w trakcie rysowania); powiązania suwaka i segmentów a pole tekstowe (osobne konteksty, klawisze niewpisujące).
- Zachowanie świadomie inne niż Solid, opisane w kodzie: `layers::forget` przy zniszczeniu otwartego `ModalState` nie przywraca fokusu (Solid przywraca przy odmontowaniu), fokus wraca na korzeń przez `on_focus_lost`.
- Resztka gpui-2: przez 100 ms zanikania Tab z korzenia może jeszcze trafić na przycisk zamykanego dialogu (jego przystanki są w kolejności Tab okna do końca zanikania); wymaga Tab i Enter w tym oknie czasu.
- Zastane (sprzed 1179cbb): brak.

## Ocena zrzutów `kit` (oba motywy)

Zrzuty `kit-headless.png` i `kit-dialog-headless.png` z `tests/kit.rs` przez renderer headless (wgpu na Mesa RADV, skala 2), po poprawce gpui-3. Kolory próbkowane w pikselach i porównane z tokenami `src/styles.css` i klasami kontrolek z `src/ui/controls` (kolory z przezroczystością składane na `--bg-1` w sRGB).

- Ciemny: tło `--bg-1` #141618, powierzchnia dialogu `--surface-solid` #202224, przyciski główny #10B981 z tekstem #06251B, drugorzędny `--surface-raised` #232528 z ramką `--border-soft` #2C2E31, duch przezroczysty z tekstem `--text-muted` #A5B0AA, niebezpieczny `error/8` #252021 z tekstem #EE9790, wyłączony główny 50% #12674C, aktywne ikonowe `favorite/12` #2C1F22 i `primary/12` #142A25, chipy (`--surface-raised`, wideo #6D28D9, GIF #1D4ED8), segmenty (tło `--surface-muted`, podświetlenie #10B981), suwak (tor `--border-strong`, wypełnienie i kciuk #10B981), przełącznik (tło `base-100`), pole (`--surface-raised`, wyłączone 0,55), pasek postępu (tor `primary/20` #13372D), alerty (`info/14` #252E33, `warning/14` #312D24, `error/14` #322728, treść `base-content/78`), tło modala `neutral/58`, nagłówek `base-content`: zgodne (różnica do 1 poziomu).
- Jasny: tło #F4F6F5, powierzchnia dialogu #FFFFFF, główny #087F5B, drugorzędny #E8EEEB, duch z tekstem #52635A, niebezpieczny #EFE7E6 z tekstem #B13C38, wyłączony główny #7EBAA8, aktywne ikonowe #EEE1E2 i #D7E8E2, tor postępu #C5DED6, alert info #D7E1E4, tło modala #747C78, nagłówek #17241E: zgodne (różnica do 1 poziomu); przed poprawką gpui-3 aktywne przyciski ikonowe i wyłączony główny były za ciemne.
- Wymiary: przycisk 32 px wysokości (`h-8`), przycisk ikonowy 36 px (`h-9`), ramka 1 px, promień 6 px (`--radius-control`), cień `--shadow-control` pod nieprzezroczystymi przyciskami; typografia: Inter, nagłówki półgrube, chipy JetBrains Mono.
- Uwagi bez niezgodności tokenów: pasek nieokreślony jest na zrzucie pusty, bo pasek animacji startuje poza torem (faza animacji, nie kolor); pierścień fokusu nie występuje na planszy (brak fokusu z klawiatury), jego geometria (`focus_ring`, `FocusRing::outset`) sprawdzona w kodzie, nie na zrzucie.

Kryterium „Zrzut `kit` w obu motywach zgodny z tokenami” uznane za spełnione po poprawce gpui-3.

## Stan

Nic nie zostaje otwarte. Ten przebieg zamknął lukę przeglądu `review-gpui` z rund 1 i 2.
