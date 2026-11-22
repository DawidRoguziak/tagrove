# E12: otwarte ustalenia przeglądu

Ocena parytetu wyglądu (agent naprawczy luk, 2026-11-22). Zamyka lukę kryterium „Parytet `search-suggestions`, `search-applied`” (docs/plans/gpui-port/E12-search.md:48): krok porównania workflow skończył się błędem, więc nikt nie ocenił par zrzutów. Teraz oba scenariusze uruchomiono trzy razy przez `job.py start E12 compare` (każdy przebieg z kodem 0, oba zrzuty w 1440×900), a pary `tauri.png` i `gpui.png` zmierzono pikselami (zrzut Tauri jest wzorcem).

## Co porównano i jak

- Wzorce: src/features/search/SearchTagatorWrapper.tsx:72 (ramka pola), src/features/search/SearchTagatorWrapper.tsx:74 (lupa), src/ui/controls/UiIcon.tsx:43 (bazowe `h-5 w-5` ikony), src/features/search/SearchTagatorWrapper.tsx:91 i src/styles.css:401 (podpowiedź Ctrl K), src/features/search/SearchTagatorWrapper.tsx:143 (separator), src/features/search/suggestions/SearchSuggestionOption.tsx:25 (wiersz podpowiedzi), src/styles.css:145 (wysokość linii 1,5), src/styles.css:231 (wiersz narzędzi).
- Metoda: obwiednie pikseli różnych od tła, profile kolumn i wierszy, próbki kolorów porównane z tokenami (`--border-strong` #2C2E31, `--surface-muted` #141618, `--surface-raised` #232528, `--color-primary` #10B981, aktywne tło podpowiedzi #1E3731).
- `search-suggestions`: ramka pola, lupa, tekst, podpowiedź Ctrl K, przycisk resetu i Search, lista podpowiedzi (położenie pod polem, szerokość, ramka, promień, cień, wysokość wierszy, aktywny wiersz z pierścieniem, kolor i pogrubienie dopasowania), wiersz narzędzi pod listą.
- `search-applied`: ramka i tekst `-cat`, kontrolka rodzaju mediów (wybrane Images), ulubione, separator, licznik „4 items”, siatka wyników (cztery obrazy).

## Zmierzone zgodnie (po poprawkach)

- Ramka pola: x 216–1244, y 9–42, ramka #2C2E31, tło #141618, w obu programach; lupa 228–243 × 19–34; Search 1169–1235 × 12–39; przycisk resetu 1137–1152 × 18–33; podpowiedź Ctrl K y 18–33 (16 px), tło #232528.
- Lista: górna ramka y 50, lewa x 253, ramka #2C2E31, tło #232528, promień i pierścień aktywnego wiersza (od y 57, kolor #10B981 na #1E3731) zgodne co do piksela w rogach; dopasowanie `ca` w kolorze #10B981, pogrubione; trzy podpowiedzi camera, car, cat w tej samej kolejności, pierwsza aktywna.
- Wiersz narzędzi: granice y 52 i 96 w obu; kontrolka rodzaju x 16–236/237, wybrany segment w kolorze primary; ulubione 32×32; licznik „4 items” 22 px wysokości; siatka: te same cztery obrazy.

## Potwierdzone ustalenia

| ID | Priorytet | Ocena | Właściciel | Ustalenie | Stan |
| --- | --- | --- | --- | --- | --- |
| e12-parity-1 | P2 | notable | E12 | W GPUI lista podpowiedzi `search-suggestions` w ogóle się nie otwierała: pole miało fokus, ale nasłuchy fokusu GPUI działają tylko w aktywnym oknie, a okno na Xvfb bez menedżera okien nigdy aktywne nie jest (aktywacja X11 wysyła tylko `_NET_ACTIVE_WINDOW`). WebKit wysyła `focusin` także w nieaktywnym oknie, więc lista Tauri była otwarta. | naprawione: src-gpui/crates/media_tagger_gpui/src/views/search.rs:293 (sesja słyszy skryptowany fokus w nieaktywnym oknie); test src-gpui/crates/media_tagger_gpui/tests/search_window.rs:404 (bez poprawki: brak listy) |
| e12-parity-2 | P3 | minor | E12 | Lupa 16 px zamiast 20 px: `h-4 w-4` wrappera przegrywa w Tauri z `h-5 w-5` z `UiIcon`, więc tekst i lista zaczynały się 4 px za wcześnie (x 249 zamiast 253). | naprawione: src-gpui/crates/media_tagger_client/src/search/layout.rs:16 i src-gpui/crates/media_tagger_client/src/search/layout.rs:30; testy src-gpui/crates/media_tagger_client/src/search/layout.rs:134 i src-gpui/crates/media_tagger_gpui/tests/search_window.rs:450 (bez poprawki: 16 zamiast 20) |
| e12-parity-3 | P3 | notable | E12 | Brak separatora przed (przyszłym) przyciskiem listy tagów: `border_l_1` na pudełku szerokim na 1 px nie rysuje się, bo shader quadów GPUI wybiera dla środkowej kolumny prawą krawędź o szerokości 0 (crates/gpui_wgpu/src/shaders.wgsl:603 w przypiętym commicie Zed); przy skali 2 rysuje się tylko lewa połowa. | naprawione: separator jako wypełnione pudełko 1 px, src-gpui/crates/media_tagger_gpui/src/views/search.rs:371; test src-gpui/crates/media_tagger_gpui/tests/search_window.rs:478 (render headless, bez poprawki: 1 z 2 kolumn urządzenia) |
| e12-parity-4 | P3 | minor | E12 | Podpowiedź Ctrl K miała 18,5 px wysokości zamiast 16: dziedziczyła wysokość linii 1,5, a skrót `font` w src/styles.css:401 ustawia `normal` (14 px dla JetBrains Mono 11 px). | naprawione: src-gpui/crates/media_tagger_client/src/search/layout.rs:44, src-gpui/crates/media_tagger_gpui/src/views/search.rs:551; testy src-gpui/crates/media_tagger_client/src/search/layout.rs:143 i src-gpui/crates/media_tagger_gpui/tests/search_window.rs:450 (bez poprawki: 18.5) |
| e12-parity-5 | P3 | minor | E12 | Ramka pola była w kolorze primary w nieaktywnym oknie; WebKit dopasowuje `focus-within` tylko w aktywnym oknie, więc zrzut Tauri ma zwykłą ramkę. | naprawione: src-gpui/crates/media_tagger_gpui/src/views/search.rs:230; test src-gpui/crates/media_tagger_gpui/tests/search_window.rs:404 (bez poprawki pada na ramce) |

Otwartych ustaleń P0–P2: brak. P0 ani P1 nie znaleziono. Różnic „notable” należących do E12: po poprawkach brak.

## Różnice zostające (minor)

- Wiersze listy mają 33 px zamiast 33,5 px (wysokość linii 19,5 px dla 13 px), więc lista kończy się na y 166 zamiast 168. Tauri rozkłada pół piksela między wiersze, GPUI zaokrągla linię tekstu; różnica do 0,5 px na wiersz. Właściciel: E12, nienaprawiane (granica zaokrąglania, nie wartość tokenu).
- Prawa krawędź listy x 1062 zamiast 1060 i podpowiedź Ctrl K 50 zamiast 52 px szerokości: szerokość tekstu „Ctrl K” w JetBrains Mono. Rasteryzacja fontów poza parytetem (D10, docs/plans/gpui-port/README.md:82).
- Separator x 289 zamiast 290, kontrolka rodzaju 1 px węższa: szerokości tekstu etykiet Inter (D10; brak zmiennych wag i `letter_spacing` w GPUI zapisany przy E06, docs/plans/gpui-port/E06-gpui-app-shell.md:135).
- Zawartość wiersza narzędzi (kontrolka rodzaju, ulubione, licznik, Bulk actions, Tile size) 1 px wyżej: wiersz ma 33 px treści dla elementów 32 px, WebKit zaokrągla środek w dół o pół piksela inaczej niż GPUI. Właściciel wiersza narzędzi: E09/E10; nienaprawiane.
- Karetka w polu jest rysowana w nieaktywnym oknie GPUI, w Tauri nie. Właściciel: E04 (`TextInput`).
- Subpikselowe obwódki kolorów na glifach podpowiedzi w GPUI: rasteryzacja fontów (D10).
- Brak przycisku listy tagów za separatorem: E13. Kolejność kafli o równych kluczach sortowania różni się między przebiegami (także między przebiegami Tauri): poza zakresem E12.

## Otwarte

Nic, co należy do E12, nie zostaje otwarte. Zostają tylko różnice minor powyżej (zaokrąglenia i fonty) oraz elementy innych epików (E04 karetka, E09/E10 wiersz narzędzi, E13 przycisk listy tagów).

## Stan

Parytet obu scenariuszy oceniony; e12-parity-1 do e12-parity-5 naprawione i potwierdzone pomiarem w drugim i trzecim przebiegu porównania.
