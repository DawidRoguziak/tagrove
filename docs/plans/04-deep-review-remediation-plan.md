# Plan napraw po dogłębnym review projektu

## Cel dokumentu

Ten dokument jest żywym planem przekazywanym między kolejnymi sesjami. Każda sesja powinna realizować jeden etap, a następnie zaktualizować ten sam plik tak, aby następna sesja znała rzeczywisty stan kodu, podjęte decyzje, odchylenia od planu i pozostałe ryzyka.

Plan obejmuje naprawy wykryte podczas przekrojowego review frontendu, backendu, warstwy danych, bezpieczeństwa operacji plikowych, wydajności, dostępności i i18n. Kolejność stawia najpierw na ochronę danych i poprawność, później na spójność kontraktów oraz wydajność, a na końcu na jakość interfejsu.

## Wiążące decyzje Jana

1. Eksport i import CSV mają identyfikować zasób wyłącznie po nazwie pliku (`file_name` / basename). To zachowanie jest celowe i nie wolno zastępować go ścieżką, fingerprintem ani innym identyfikatorem.
2. Jeżeli w bibliotece istnieje kilka zasobów o tej samej nazwie, import może zastosować wpis do wszystkich pasujących zasobów. Ten fan-out jest częścią obecnej semantyki i ma zostać jawnie udokumentowany, a nie usunięty.
3. Punkt 18 z review — utwardzanie CSP, `assetProtocol` i ustawień renderera/SmartScreen — jest wyłączony z zakresu. Nie implementować go w ramach tego planu.
4. Bez osobnego, wyraźnego polecenia Jana nie wolno tworzyć testów, aktualizować istniejących testów ani uruchamiać testów, buildów i innych poleceń weryfikacyjnych.

## Status całego planu

- Ostatnia aktualizacja: 2026-07-15
- Aktualny etap: Etap 1 ukończony
- Następna czynność: w kolejnej sesji rozpocząć Etap 2 zgodnie z protokołem planu
- Ukończone etapy: Etap 1
- Otwarte blokery: brak

| Etap | Zakres | Status | Zależności |
| --- | --- | --- | --- |
| 1 | Bezpieczne skanowanie i ochrona przed równoległymi instancjami | Ukończony | brak |
| 2 | Bezpieczne usuwanie i atomowe rozwiązywanie duplikatów | Oczekuje | Etap 1 |
| 3 | Walidacja i ograniczenie zaufania przy imporcie backupu | Oczekuje | Etap 1 |
| 4 | Spójny eksport/restore backupu i tryb maintenance bazy | Oczekuje | Etap 3 |
| 5 | Kolejność zapytań galerii i unieważnianie wyników | Oczekuje | Etap 1 |
| 6 | Rozdzielenie summary/details, cache epoch i selekcja | Oczekuje | Etap 5 |
| 7 | Wersjonowanie miniaturek i nieblokujące komendy | Oczekuje | Etapy 1 i 5 |
| 8 | Atomowy import CSV przy zachowaniu identyfikacji po nazwie | Oczekuje | Etap 4 |
| 9 | Skalowalność zapytań i ograniczenia dekodowania obrazów | Oczekuje | Etapy 5–7 |
| 10 | Stos modali, klawiatura i dostępność | Oczekuje | Etap 6 |
| 11 | Spójność tagów, paginacja tagów i i18n | Oczekuje | Etapy 6 i 8 |
| 12 | Audyt końcowy i zamknięcie planu | Oczekuje | Etapy 1–11 |

## Obowiązkowy protokół każdej kolejnej sesji

### Na początku sesji

1. Przeczytać całe `AGENTS.md`, ten dokument oraz `docs/architecture/system-overview.md`, jeżeli etap dotyka backendu.
2. Sprawdzić aktualny kod i `git status`; kod jest źródłem prawdy, ale każdą rozbieżność z planem trzeba odnotować w tym dokumencie.
3. Wybrać pierwszy etap ze statusem `Oczekuje` albo kontynuować etap `W toku`. Nie rozpoczynać późniejszych etapów bez polecenia Jana.
4. Przed pierwszą zmianą ustawić status etapu na `W toku`, uzupełnić datę rozpoczęcia i krótki wpis w jego dzienniku przekazania.
5. Jeżeli etap zmienia kontrakt frontend–backend, najpierw zaktualizować współdzielone typy, a następnie backend, serwisy, hooki i UI.
6. Zachować cienkie komendy Tauri, SQL w `src-tauri/src/db.rs`, workflow w `src-tauri/src/services/*` oraz kolejność locków `scan_lock`, potem `thumb_lock`.
7. Nie zmieniać testów i nie uruchamiać komend testowych/buildów bez osobnego polecenia Jana. Samo istnienie kryteriów weryfikacji w planie nie jest taką zgodą.

### W trakcie sesji

1. Oznaczać ukończone zadania danego etapu przez `[x]`; nie usuwać pierwotnych punktów ani historii.
2. Jeżeli odkrycie zmienia zakres, zależności lub kolejność, zaktualizować plan w tej samej sesji przed przejściem dalej.
3. Konieczne prace wykraczające poza bieżący etap ograniczyć do minimum i opisać w sekcji `Odchylenia i decyzje`.
4. Nie poprawiać przy okazji punktu 18 ani identyfikacji CSV po nazwie pliku.
5. Nową trwałą zasadę projektu dopisać do `AGENTS.md`; nie pozostawiać jej wyłącznie jako ukrytej wiedzy sesji.

### Przed zakończeniem sesji lub etapu

1. Zaktualizować status etapu:
   - `Ukończony` tylko po spełnieniu wszystkich kryteriów odbioru,
   - `W toku`, jeżeli pozostała praca,
   - `Zablokowany`, jeżeli istnieje konkretny blocker wymagający decyzji lub zmiany zewnętrznej.
2. Uzupełnić dziennik etapu o:
   - datę,
   - zmienione pliki,
   - rzeczywiście wdrożone zachowanie,
   - decyzje i odchylenia,
   - migracje lub wpływ na kompatybilność,
   - testy utworzone/zaktualizowane/uruchomione wyłącznie wtedy, gdy Jan wydał takie polecenie,
   - niewykonane sprawdzenia i powód,
   - znane ryzyka oraz dokładny następny krok.
3. Zaktualizować tabelę `Status całego planu`, pole `Ostatnia aktualizacja`, `Aktualny etap`, `Następna czynność` i listę blockerów.
4. Jeżeli etap jest nieukończony, pozostawić szczegółowy handoff pozwalający wznowić pracę bez ponownego odkrywania kontekstu.
5. Nie oznaczać etapu jako ukończonego wyłącznie dlatego, że skończył się czas sesji.

## Wspólne zasady implementacyjne

- Priorytetem są: brak utraty danych, atomowość operacji, responsywność UI i możliwość bezpiecznego wznowienia po błędzie.
- Operacje plikowe i ciężkie zapytania nie mogą blokować głównego wątku Tauri ani Reacta.
- Mutacja danych oraz powiązane zwiększenie revision powinny należeć do tej samej transakcji.
- Przy zmianach schematu lub formatu danych zachować kompatybilność wsteczną albo opisać i wdrożyć migrację.
- Publiczne typy należy rozszerzać w istniejących `src/types.ts` i `src/components/app/types.ts`, zamiast tworzyć równoległe modele.
- Nazwa eventu progress pozostaje `process-progress`, a payload pozostaje zgodny z `ScanProgress`, chyba że Jan osobno zatwierdzi zmianę kontraktu.
- Nie zmieniać kluczy persistence `media-tagger.theme` i `media-tagger.language` ani zakresu motywów `light`/`dark`.
- W miejscach dotykających wielu rekordów preferować jedną operację backendową z walidacją całości i transakcją zamiast sekwencji niezależnych komend z frontendu.
- Weryfikację wydajności zaczynać od pomiaru istniejącego zachowania. Nie wprowadzać dużej przebudowy wyłącznie na podstawie intuicji.

---

## Etap 1 — Bezpieczne skanowanie i ochrona przed równoległymi instancjami

**Status:** Ukończony  
**Rozpoczęto:** 2026-07-15  
**Ukończono:** 2026-07-15

### Cel

Nie dopuścić, aby częściowo nieudany discovery usuwał poprawne rekordy, oraz wyeliminować możliwość równoczesnych procesów modyfikujących tę samą bazę i mapowania skanu.

### Główne obszary

- `src-tauri/src/indexer.rs`
- `src-tauri/src/services/scan_service.rs`
- `src-tauri/src/db.rs`
- inicjalizacja aplikacji Tauri i konfiguracja single-instance/interprocess lock

### Zadania

- [x] Zastąpić ciche odrzucanie błędów `WalkDir` i metadanych raportowaniem kompletności discovery.
- [x] Wprowadzić wynik discovery rozróżniający pełny skan, częściowy skan i błąd krytyczny.
- [x] Uruchamiać destructive cleanup/prune wyłącznie po potwierdzonym pełnym skanie danego root/generation.
- [x] Przy częściowym skanie zachować dotychczasowe rekordy i zwrócić/wyemitować czytelne podsumowanie ostrzeżeń.
- [x] Sprawdzić generacyjne mapowanie scan rootów i zabezpieczyć je przed wzajemnym czyszczeniem.
- [x] Wymusić jedną aktywną instancję dla danego profilu danych albo dodać równoważny międzyprocesowy lock obejmujący mutacje bazy i skany.
- [x] Zachować izolację identyfikatorów dev, E2E i release opisaną w `AGENTS.md`.
- [x] Jeżeli zmienia się payload summary, najpierw rozszerzyć współdzielony typ i zachować kompatybilność konsumentów.

### Kryteria odbioru

- Błąd odczytu jednego katalogu/pliku nie powoduje usunięcia wcześniej zindeksowanych zasobów z niedostępnego fragmentu drzewa.
- Cleanup wykonuje się tylko przy jednoznacznie kompletnym discovery.
- Druga instancja używająca tego samego katalogu danych nie rozpoczyna mutującej pracy.
- Użytkownik dostaje rozróżnienie między sukcesem pełnym a skanem częściowym.
- Komendy pozostają cienkie, a workflow skanu pozostaje w serwisie.

### Dziennik przekazania

### 2026-07-15 — rozpoczęcie sesji implementacyjnej

- Status: W toku.
- Rozpoczęto audyt discovery, cleanup generacyjnego i ochrony przed równoległymi instancjami.
- Nie uruchamiano testów, buildów ani innych poleceń weryfikacyjnych — brak osobnego polecenia Jana.

### 2026-07-15 — sesja implementacyjna

- Status po sesji: Ukończony.
- Zmienione pliki:
  - `src/types.ts`
  - `src/components/settings/hooks/useScanSettingsActions.ts`
  - `src/i18n/locales/{en,pl,fr,de,it,es,ru,zh,ja,ko,cs}.json`
  - `src-tauri/src/models.rs`
  - `src-tauri/src/indexer.rs`
  - `src-tauri/src/services/scan_service.rs`
  - `src-tauri/src/db.rs`
  - `src-tauri/src/app/instance_lock.rs`
  - `src-tauri/src/app/mod.rs`
  - `src-tauri/src/lib.rs`
  - `src-tauri/Cargo.toml`
  - `src-tauri/Cargo.lock`
  - `docs/architecture/system-overview.md`
  - `AGENTS.md`
  - `docs/plans/04-deep-review-remediation-plan.md`
- Wdrożone:
  - `WalkDir` raportuje liczbę błędów przejścia i kompletność discovery zamiast cicho odrzucać błędy.
  - Błędy przejścia drzewa, fingerprintu/metadanych i indeksowania oznaczają root jako częściowy; błąd infrastrukturalny callbacku/DB/workera nadal kończy komendę przez `Err` jako błąd krytyczny.
  - Prune mapowań `asset_scan_roots` wykonuje się wyłącznie po pełnym discovery bez błędów metadanych i indeksowania. Przy skanie częściowym stare mapowania oraz rekordy pozostają zachowane.
  - SQL cleanup pozostał ograniczony dokładnym `root_path`; jawna nazwa `prune_completed_scan_root_generation` dokumentuje wymagany warunek wywołania i nie pozwala rootom czyścić cudzych mapowań.
  - `ScanSummary` zwraca `completion: complete | partial`, a UI pokazuje osobny lokalizowany komunikat częściowego skanu z informacją o pominiętym cleanupie i zachowaniu rekordów.
  - Bootstrap zdobywa wyłączny międzyprocesowy lock `instance.lock` w rzeczywistym `app_data_dir` przed otwarciem bazy i trzyma jego uchwyt przez cały czas życia aplikacji.
  - Profile dev, E2E i release zachowują niezależne locki dzięki istniejącym, różnym identyfikatorom Tauri i katalogom danych.
- Decyzje i odchylenia:
  - Zamiast polegać wyłącznie na pluginie single-instance zastosowano lock pliku profilu przez `fs2`, ponieważ bezpośrednio chroni bazę także w wąskim oknie równoległego startu procesów.
  - Pole `completion` jest obowiązkowe w odpowiedzi Rust, ale opcjonalne w typie TypeScript. Brak pola jest traktowany jak pełny sukces, co zachowuje zgodność starszych mocków i odpowiedzi bez aktualizacji testów.
  - Nie zmieniono nazwy ani payloadu eventu `process-progress`; końcowa kompletność jest częścią `ScanSummary`.
  - Nie zmieniono konfiguracji identyfikatorów, semantyki CSV ani punktu 18 wyłączonego z zakresu.
- Migracje / kompatybilność:
  - Brak migracji bazy danych i zmian schematu.
  - Dodano zależność `fs2 0.4.3`; `Cargo.lock` zaktualizowano przez `cargo fetch` bez uruchamiania kompilacji.
  - Rozszerzenie JSON summary jest addytywne; dotychczasowe pola zachowują znaczenie.
- Testy utworzone lub zaktualizowane:
  - Nie wykonywano — brak osobnego polecenia Jana.
- Uruchomione sprawdzenia:
  - Nie uruchamiano testów, buildów, kompilacji ani aplikacji — brak osobnego polecenia Jana.
  - Wykonano wyłącznie read-only audyt kodu/diffu oraz `cargo fetch` potrzebne do deterministycznej aktualizacji lockfile zależności.
- Niewykonane sprawdzenia i ryzyka:
  - Nie potwierdzono kompilacją poprawności Rust/TypeScript ani parsowania zasobów JSON.
  - Nie wykonano ręcznej próby równoległego startu dwóch instancji ani skanu drzewa z kontrolowanym błędem uprawnień.
  - Krytyczny błąd DB po zatwierdzeniu wcześniejszego batcha może nadal pozostawić bezpieczny, częściowy zapis bez końcowego bump revision; destructive prune nie wykona się, ale pełna transakcyjność całego skanu pozostaje poza zakresem tego etapu.
- Następny dokładny krok:
  - W kolejnej sesji rozpocząć Etap 2 i przed pierwszą zmianą oznaczyć go jako `W toku`; testy/build uruchomić tylko po osobnym poleceniu Jana.

### Odchylenia i decyzje

- `completion` dodano jako addytywne pole summary zamiast zmiany kontraktu progress.
- Ochronę single-instance zrealizowano przez lock rzeczywistego profilu danych, nie przez sam plugin Tauri.

---

## Etap 2 — Bezpieczne usuwanie i atomowe rozwiązywanie duplikatów

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zapewnić zgodność bazy z systemem plików po usuwaniu oraz zastąpić frontendową sekwencję niezależnych rename/delete jedną kontrolowaną operacją backendową.

### Główne obszary

- `src-tauri/src/commands/assets.rs`
- `src-tauri/src/services/*` — nowy lub istniejący serwis mutacji plikowych
- `src-tauri/src/db.rs`
- akcje lightboxa i rozwiązywania duplikatów w `src/components/*`
- współdzielone typy summary/result

### Zadania

- [ ] Ustalić i udokumentować kolejność operacji usunięcia: walidacja, operacja plikowa, transakcja DB i kompensacja.
- [ ] Nie ignorować błędu usunięcia pliku; zwracać wynik odróżniający brak pliku, sukces i błąd systemu plików.
- [ ] Nie usuwać rekordu DB, jeśli plik miał zostać usunięty, ale operacja plikowa nie powiodła się i nie wykonano bezpiecznej kompensacji.
- [ ] Frontend ma honorować pola wyniku, w tym `removed_media_file`, i odświeżać stan tylko zgodnie z faktycznym rezultatem.
- [ ] Zaprojektować jeden kontrakt batch dla rozwiązywania duplikatów: plan zachowania, rename, delete i oczekiwane wersje rekordów.
- [ ] Walidować cały batch przed pierwszą destrukcyjną zmianą.
- [ ] Wykonać batch pod właściwymi lockami, z transakcją DB i kontrolowaną kompensacją lub bezpiecznym mechanizmem kosza/stagingu.
- [ ] Przenieść nietrywialny workflow z komend do serwisu; SQL pozostawić w `db.rs`.

### Kryteria odbioru

- Po błędzie usuwania pliku rekord nie znika bez śladu z bazy.
- UI nie raportuje pełnego sukcesu, kiedy plik pozostał na dysku.
- Batch duplikatów jest walidowany w całości przed mutacją i nie pozostawia nieopisanego pół-stanu po błędzie pośrodku.
- Revision i mutacje DB są atomowe.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 3 — Walidacja i ograniczenie zaufania przy imporcie backupu

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Traktować bundle ZIP jako niezaufane wejście i nie dopuścić do podmiany poprawnej bazy pustym/uszkodzonym plikiem ani do zapisania niebezpiecznych ścieżek miniaturek.

### Główne obszary

- `src-tauri/src/services/backup_service.rs`
- `src-tauri/src/db.rs`
- `src-tauri/src/services/thumb_service.rs`
- typy `DbBundleImportSummary` i powiązane błędy

### Zadania

- [ ] Dodać limity liczby wpisów, rozmiaru skompresowanego i rozpakowanego, rozmiaru pojedynczego wpisu oraz rozsądny limit współczynnika kompresji.
- [ ] Odrzucać duplikaty krytycznych wpisów, nietypowe typy wpisów i nieoczekiwane nazwy/ścieżki.
- [ ] Zweryfikować `media.db` przed podmianą: minimalny rozmiar, nagłówek SQLite, możliwość otwarcia read-only, `quick_check`/odpowiednik oraz wymagany schemat/migracje.
- [ ] Nie pozwalać, aby plik zero-byte został automatycznie zainicjalizowany jako nowa poprawna baza podczas importu.
- [ ] Zweryfikować wszystkie importowane `thumb_path`; po imporcie ścieżki mają wskazywać wyłącznie do kontrolowanego katalogu miniaturek albo zostać bezpiecznie przepisane/wyzerowane.
- [ ] Utwardzić helpery usuwania miniaturek tak, aby nigdy nie usuwały ścieżek poza dozwolonym rootem, nawet jeśli baza jest uszkodzona.
- [ ] Walidację wykonać na stagingu przed dotknięciem aktywnej bazy i aktywnego katalogu miniaturek.
- [ ] Zwracać precyzyjne, nieujawniające zbędnych ścieżek błędy importu.

### Kryteria odbioru

- Pusty, uszkodzony albo schematycznie niezgodny `media.db` nie zastępuje aktywnej bazy.
- ZIP przekraczający limity jest odrzucany przed niekontrolowanym zużyciem dysku/pamięci.
- Żadna ścieżka pochodząca z importowanej bazy nie pozwala usunąć pliku poza katalogiem miniaturek.
- Aktywne dane nie są modyfikowane, dopóki staging nie przejdzie pełnej walidacji.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 4 — Spójny eksport/restore backupu i tryb maintenance bazy

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zapewnić spójny snapshot SQLite oraz crash-safe restore obejmujący bazę i miniaturki, bez kolizji z równoległymi mutacjami.

### Główne obszary

- `src-tauri/src/services/backup_service.rs`
- `src-tauri/src/services/locks.rs` lub właściwy moduł locków
- `src-tauri/src/db.rs` i zarządzanie połączeniami
- komendy backup/restore

### Zadania

- [ ] Wprowadzić globalny tryb maintenance/lock dla operacji wymagających stabilnego obrazu bazy; objąć nim także mutacje tagów, favorite, grup i plików.
- [ ] Zachować obowiązującą kolejność locków `scan_lock`, potem `thumb_lock`, i jawnie opisać pozycję maintenance locka, aby uniknąć deadlocków.
- [ ] Zastąpić sekwencyjne kopiowanie DB/WAL/SHM spójnym mechanizmem SQLite Backup API, `VACUUM INTO` lub innym poprawnym snapshotem.
- [ ] Eksportować do unikalnego pliku tymczasowego w bezpiecznej lokalizacji i atomowo publikować wynik.
- [ ] Odrzucać target będący aktywną bazą, plikiem wewnątrz katalogu miniaturek albo inną ścieżką powodującą samodołączenie/kolizję.
- [ ] Przed restore zatrzymać nowe operacje, doprowadzić połączenia/pool do bezpiecznego stanu i zapisać journal operacji.
- [ ] Restore wykonać przez staging i atomową zamianę; poprzednią wersję zachować do czasu udanej walidacji oraz ponownego otwarcia.
- [ ] Dodać procedurę odzyskania po przerwaniu procesu między rename'ami i wykonać ją przy następnym starcie.
- [ ] Po udanym restore jawnie unieważnić wszystkie backendowe i frontendowe cache zależne od ID/revision.

### Kryteria odbioru

- Eksport reprezentuje jeden spójny punkt w czasie mimo WAL.
- Równoległa mutacja nie może wejść w trakcie snapshotu/restore.
- Awaria w dowolnym momencie restore pozostawia możliwą do odzyskania poprzednią lub nową kompletną wersję, a nie nieokreślony pół-stan.
- Target backupu nie może wskazywać aktywnych danych aplikacji.
- Po restore żadne stare połączenie ani cache nie odczytuje poprzedniego obrazu danych.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 5 — Kolejność zapytań galerii i unieważnianie wyników

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Usunąć wyścig generacji zapytań, przez który nowsze wyszukiwanie może zostać uznane za przestarzałe, oraz zapewnić poprawne odświeżanie aktywnego filtra po mutacjach.

### Główne obszary

- `src-tauri/src/commands/assets.rs`
- `src-tauri/src/services/asset_query_service.rs`
- `src/hooks/useLibraryAssets.ts`
- akcje mutujące tagi, favorite i media group
- współdzielone typy request/session/revision

### Zadania

- [ ] Ujednolicić kontrakt generacji/request ID frontend–backend i nie ignorować generacji przesyłanej przez frontend.
- [ ] Zarejestrować kolejność żądania przed delegacją do `spawn_blocking`, tak aby scheduler nie mógł odwrócić semantyki latest-request-wins.
- [ ] Zwracać jednoznaczny wynik: aktualny, anulowany/superseded albo błąd; frontend nie może zamieniać superseded na pustą galerię.
- [ ] Dodać kooperacyjne sprawdzanie anulowania w kosztownych etapach zapytania, nie tylko po wykonaniu całej pracy.
- [ ] Zdefiniować centralną ścieżkę invalidation po zmianie tagów, favorite, grupy, nazwy i usunięciu.
- [ ] Po mutacji wpływającej na aktywny filtr ponownie ocenić membership i ordering, zamiast tylko patchować obiekt w miejscu.
- [ ] Powiązać query sessions z revision/cache epoch, aby restore lub duża mutacja unieważniały wszystkie stare wyniki.

### Kryteria odbioru

- Nowsze zapytanie nigdy nie jest odrzucane z powodu późniejszego wejścia starszego zadania do `spawn_blocking`.
- Superseded request nie czyści prawidłowej galerii.
- Zmiana tagów/favorite/group natychmiast zapewnia poprawny membership i kolejność dla aktywnego filtra.
- Restore/revision epoch uniemożliwia użycie starej sesji zapytania.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 6 — Rozdzielenie summary/details, cache epoch i selekcja

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Usunąć fałszywe pełne obiekty `Asset`, zabezpieczyć cache przed ponownym użyciem ID po restore i naprawić selekcję opartą o pomieszane indeksy globalne/lokalne.

### Główne obszary

- `src/types.ts`
- `src/components/app/types.ts`
- `src/hooks/useLibraryAssets.ts`
- `src/hooks/useSelectionState.ts`
- `src/hooks/useLibraryLifecycle.ts`
- `src/components/bulk/*`

### Zadania

- [ ] Wprowadzić jawne typy `AssetSummary` i `AssetDetails` albo równoważny discriminated model; nie fabrykować path, file_name, tags i size wartościami sugerującymi kompletność.
- [ ] Dostosować komponenty tak, aby operacje wymagające details czekały na pełny rekord lub pokazywały stan ładowania.
- [ ] Nie pozwalać, by późno doładowany detail nadpisał nowszą lokalną mutację; użyć revision/version per rekord lub kontrolowanego merge.
- [ ] Dodać cache epoch związany z obrazem bazy i czyścić details, summary, selection i query sessions po restore/reopen.
- [ ] Przechowywać selekcję po stabilnych ID, a anchor/range liczyć względem jednego jawnego porządku całego wyniku.
- [ ] Nie uzależniać poprawności shift-select od aktualnie utrzymanego okna cache.
- [ ] Eviction danych widoku nie może samoczynnie usuwać wyboru; oddzielić lifecycle selection od lifecycle render cache.
- [ ] Zdefiniować zachowanie, gdy wybrany rekord znika z aktywnego filtra albo zostaje usunięty.

### Kryteria odbioru

- Kod typów nie przedstawia summary jako kompletnego `Asset`.
- Mutacja wykonana przed załadowaniem details nie jest później cofana przez stary response.
- Po restore nie można zobaczyć ścieżki/tagów poprzedniego rekordu o ponownie użytym ID.
- Shift-select wybiera poprawny zakres także przy wirtualizacji, paginacji i eviction.
- Zaznaczenie nie znika tylko dlatego, że element wypadł z cache renderowania.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 7 — Wersjonowanie miniaturek i nieblokujące komendy

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zapobiec przypisaniu miniatury wygenerowanej dla starej wersji pliku oraz usunąć blokowanie głównego wątku przez synchroniczne komendy Tauri.

### Główne obszary

- `src-tauri/src/thumbs.rs`
- `src-tauri/src/services/thumb_*`
- `src-tauri/src/commands/thumbs.rs`
- `src-tauri/src/commands/assets.rs`
- `src-tauri/src/db.rs`

### Zadania

- [ ] Ujednolicić wersję źródła używaną przez skan i thumbnail pipeline; użyć co najmniej tego samego fingerprintu o precyzji nanosekundowej i rozmiaru pliku.
- [ ] Zapisać oczekiwaną wersję przy zleceniu i warunkowo aktualizować DB tylko wtedy, gdy rekord nadal wskazuje tę samą wersję źródła.
- [ ] Publikować plik thumbnaila atomowo z temp file i usuwać wynik osierocony po przegranym compare-and-set.
- [ ] Włączyć `request_id`/generation do faktycznego anulowania lub ignorowania starego backendowego strumienia pracy.
- [ ] Przejrzeć `remove_scan_root`, `delete_asset`, `rename_asset_file`, legacy `list_assets` i inne synchroniczne komendy wykonujące I/O lub czekające na lock.
- [ ] Zmienić ciężkie komendy na `async`, a blokujące FS/SQLite przenieść do `spawn_blocking`/serwisu bez trzymania niepotrzebnych locków przez await.
- [ ] Zachować cienkie granice komend i mapowanie `AppError` do `Result<T, String>`.

### Kryteria odbioru

- Thumbnail starej wersji pliku nie może zostać przypisany nowej wersji rekordu.
- Reset/nowy request nie pozwala staremu pipeline'owi nadpisywać aktualnego stanu.
- Komendy z operacjami I/O i oczekiwaniem na lock nie blokują głównego wątku Tauri.
- Pliki tymczasowe i osierocone wyniki są sprzątane w kontrolowany sposób.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 8 — Atomowy import CSV przy zachowaniu identyfikacji po nazwie

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Uodpornić import CSV na uszkodzone nagłówki i błędy w połowie pliku, bez zmiany celowej semantyki identyfikacji zasobów wyłącznie po basename.

### Nienegocjowalny kontrakt

- CSV zapisuje nazwę pliku, nie pełną ścieżkę.
- Import dopasowuje po nazwie pliku.
- Wiele zasobów o tej samej nazwie może otrzymać ten sam zestaw tagów.
- Nie dodawać path, fingerprintu ani ukrytego ID do formatu bez nowej, wyraźnej decyzji Jana.

### Główne obszary

- `src-tauri/src/commands/import_export.rs`
- nowy lub istniejący serwis import/export
- `src-tauri/src/db.rs`
- typy `CsvExportSummary` i `CsvImportSummary`
- dokumentacja formatu CSV widoczna dla użytkownika

### Zadania

- [ ] Walidować wymagane nazwy nagłówków i odrzucać brakujące/zdublowane kolumny zamiast domyślnie używać indeksów 0/1.
- [ ] Sparsować i zwalidować cały dokument przed pierwszą mutacją DB.
- [ ] Zdefiniować zachowanie dla pustej nazwy, pustych tagów, duplikatów tagów, nieznanych kolumn i wielokrotnych wpisów tej samej nazwy.
- [ ] Zastosować cały import w jednej transakcji albo jawnych, raportowanych chunkach z semantyką all-or-nothing; preferowane all-or-nothing dla obecnego formatu.
- [ ] Zwiększyć revision w tej samej transakcji co import.
- [ ] Przenieść workflow z komendy do serwisu, a SQL zachować w `db.rs`.
- [ ] Zachować dotychczasowe kolumny i eksport basename-only.
- [ ] Jawnie opisać w UI/dokumentacji, że taka sama nazwa w wielu katalogach jest celowo traktowana jako wspólny klucz importu.

### Kryteria odbioru

- Nieprawidłowy nagłówek lub błąd w późnym wierszu nie pozostawia częściowo zaimportowanych tagów.
- Revision odpowiada dokładnie zatwierdzonej transakcji importu.
- Eksport nadal zawiera wyłącznie nazwę pliku jako identyfikator zasobu.
- Import nadal stosuje wpis do wszystkich rekordów o pasującej nazwie.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Basename-only i fan-out są świadomą decyzją produktu, a nie defektem do usunięcia.

---

## Etap 9 — Skalowalność zapytań i ograniczenia dekodowania obrazów

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zmniejszyć koszt pamięci i CPU dla bardzo dużych bibliotek oraz ograniczyć ryzyko nadmiernej alokacji podczas generowania miniaturek.

### Główne obszary

- `src-tauri/src/services/asset_query_service.rs`
- `src-tauri/src/db.rs`
- `src-tauri/src/thumbs.rs`
- scheduler workerów thumbnaili
- frontendowa paginacja/wirtualizacja

### Zadania

- [ ] Przed przebudową zebrać baseline na reprezentatywnych rozmiarach biblioteki, ale tylko jeśli Jan osobno pozwoli uruchomić odpowiednie komendy/pomiary.
- [ ] Ograniczyć materializowanie i sortowanie pełnej listy ID per filtr; rozważyć keyset pagination, tymczasową/session table albo chunkowane snapshoty.
- [ ] Ograniczyć liczbę i pamięć przechowywanych query sessions według realnego budżetu, a nie tylko stałej liczby sesji.
- [ ] Wykonywać cooperative cancellation podczas kosztownego SQL/post-processingu.
- [ ] Zweryfikować indeksy SQLite dla dominujących filtrów AND/include/exclude/meta i ordering.
- [ ] Przed pełnym decode sprawdzać format, wymiary i przewidywany koszt; ustawić maksymalną liczbę pikseli/bytes oraz czytelny błąd dla przekroczonych limitów.
- [ ] Dostosować współbieżność workerów do budżetu pamięci, nie tylko liczby rdzeni.
- [ ] Zachować responsywność UI i backpressure między kolejką frontendu a backendem.

### Kryteria odbioru

- Pamięć query sessions ma jawny, egzekwowany limit.
- Anulowane zapytanie przestaje zużywać znaczące CPU możliwie wcześnie.
- Złośliwy lub skrajnie duży obraz nie powoduje nieograniczonej alokacji w wielu workerach.
- Paginacja zachowuje stabilną kolejność i poprawność przy równoległych mutacjach/revision.
- Po autoryzowanym pomiarze plan zawiera wyniki przed/po; bez autoryzacji wyraźnie zapisuje brak pomiaru.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 10 — Stos modali, klawiatura i dostępność

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zapewnić, że tylko najwyższa warstwa reaguje na Escape, fokus pozostaje w aktywnym dialogu i wraca do elementu wywołującego, a wyszukiwalne kontrolki mają kompletną semantykę klawiatury/ARIA.

### Główne obszary

- komponenty modalne lightboxa, bulk, settings i tag list
- wspólne primitive dialog/popover
- autocomplete/combobox tagów

### Zadania

- [ ] Zmapować wszystkie globalne listenery `keydown` i kolejność zagnieżdżonych warstw.
- [ ] Wprowadzić jeden mechanizm stosu warstw; Escape i backdrop zamykają wyłącznie najwyższą aktywną warstwę.
- [ ] Dodać focus trap, początkowy fokus, przywracanie fokusu oraz inert/aria-hidden dla tła zgodnie z używanym primitive.
- [ ] Ujednolicić role, `aria-modal`, etykietowanie i opis dialogów.
- [ ] Dokończyć semantykę combobox/listbox: expanded, controls, active descendant, wybór strzałkami, Enter, Escape i powrót fokusu.
- [ ] Nie zmieniać pozycji i kierunku wizualnego UI bez osobnej potrzeby; używać istniejących tokenów DaisyUI i zmiennych cieni.
- [ ] Sprawdzić zachowanie desktop/mobile ręcznie tylko jeśli Jan wyraźnie zleci uruchomienie aplikacji lub builda.

### Kryteria odbioru

- Jeden Escape nie zamyka kilku zagnieżdżonych warstw.
- Fokus nie wychodzi poza aktywny dialog i wraca do logicznego triggera po zamknięciu.
- Główne operacje tagowania i lightboxa są wykonalne klawiaturą.
- Kontrolki autocomplete wystawiają spójną semantykę combobox/listbox.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Etap 11 — Spójność tagów, paginacja tagów i i18n

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Ustanowić jeden invariant tagu we wszystkich przepływach, usunąć ukryty limit listy znanych tagów i doprowadzić zasoby językowe oraz generator do sprawdzalnej spójności.

### Główne obszary

- UI edycji pojedynczej i bulk tagów
- search parser oraz import CSV
- backendowa walidacja i API listy tagów
- `src/i18n/*` i generator zasobów
- wszystkie wspierane języki `AppLanguage`

### Zadania

- [ ] Potwierdzić invariant tagu zgodny z obecną semantyką wyszukiwania i CSV; domyślnie przyjąć tag bez whitespace, chyba że Jan wybierze model z pełnym escapingiem/quotingiem.
- [ ] Współdzielić normalizację/walidację między UI, bulk, search, CSV i backendem.
- [ ] Backend ma egzekwować invariant niezależnie od klienta i zwracać precyzyjny błąd.
- [ ] Usunąć rozjazd frontendu proszącego o 500 tagów i backendu obcinającego do 200; dodać paginację albo jawny endpoint pełnej listy z kontrolowanym limitem.
- [ ] Uzupełnić brakujące klucze względem `en`, w tym `common.confirm`, we wszystkich językach.
- [ ] Naprawić walidację namespace języka `cs` i objąć nią wszystkie wartości `AppLanguage`.
- [ ] Naprawić generator tak, aby nie polegał na nieaktualnym `} as const;`, nie wykonywał niebezpiecznego `eval` importowanych zmiennych i nie pomijał `cs`.
- [ ] Walidować zgodność placeholderów interpolacji między językami.
- [ ] Nie zmieniać inicjalizacji języka ani klucza `media-tagger.language`.

### Kryteria odbioru

- Ten sam tekst tagu jest akceptowany albo odrzucany identycznie w każdym przepływie.
- Lista znanych tagów nie ucina niewidocznie pozycji 201+.
- Każdy język z `AppLanguage` ma komplet wymaganych kluczy i zgodne placeholdery.
- Generator działa na aktualnym formacie źródeł i nie wykonuje kodu z plików tłumaczeń.
- Brakujące tłumaczenie nie ujawnia surowego klucza w normalnym UI.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Jeżeli tagi z whitespace mają być zachowane, etap wymaga osobnej decyzji o quoting/escaping w search i CSV przed implementacją.

---

## Etap 12 — Audyt końcowy i zamknięcie planu

**Status:** Oczekuje  
**Rozpoczęto:** —  
**Ukończono:** —

### Cel

Zweryfikować, że wszystkie etapy tworzą spójny system, nie przywróciły wcześniejszych wyścigów i mają kompletny handoff.

### Zadania

- [ ] Przejrzeć wszystkie kryteria odbioru etapów 1–11 i porównać je z aktualnym kodem.
- [ ] Sprawdzić zgodność publicznych typów frontend–backend, registration komend i granic services/db.
- [ ] Sprawdzić kolejność locków oraz brak nowych sync I/O na głównym wątku.
- [ ] Sprawdzić atomowość revision dla wszystkich zmienionych mutacji.
- [ ] Sprawdzić invalidation po scan, delete, rename, tag, favorite, group, CSV import i restore.
- [ ] Potwierdzić, że CSV nadal jest basename-only i że punkt 18 nie został przypadkowo włączony do zmian.
- [ ] Wykonać testy/build/E2E wyłącznie na podstawie osobnego, wyraźnego polecenia Jana i wyłącznie przez skrypty z `package.json`; nigdy przez `bunx`.
- [ ] Udokumentować wszystkie niewykonane sprawdzenia, znane ograniczenia i świadomie zaakceptowane ryzyka.
- [ ] Przenieść rzeczywiście trwałe reguły do `AGENTS.md` i oznaczyć plan jako zakończony.

### Kryteria odbioru

- Każdy etap ma status, daty i kompletny dziennik zmian.
- Nie istnieje nierozliczone kryterium odbioru bez jawnej decyzji/ryzyka.
- Dokument wskazuje faktycznie wykonane i niewykonane weryfikacje.
- `Status całego planu` wskazuje brak dalszego etapu albo precyzyjny follow-up poza zakresem.

### Dziennik przekazania

- Brak wpisów.

### Odchylenia i decyzje

- Brak.

---

## Szablon wpisu po sesji

Skopiować poniższy blok do `Dziennik przekazania` realizowanego etapu i uzupełnić go przed zakończeniem sesji:

```md
### YYYY-MM-DD — sesja implementacyjna

- Status po sesji: W toku / Ukończony / Zablokowany
- Zmienione pliki:
  - `ścieżka/do/pliku`
- Wdrożone:
  - ...
- Decyzje i odchylenia:
  - ...
- Migracje / kompatybilność:
  - ...
- Testy utworzone lub zaktualizowane:
  - Nie wykonywano — brak osobnego polecenia Jana. / ...
- Uruchomione sprawdzenia:
  - Nie uruchamiano — brak osobnego polecenia Jana. / `dokładne polecenie` — wynik
- Niewykonane sprawdzenia i ryzyka:
  - ...
- Następny dokładny krok:
  - ...
```

## Poza zakresem tego planu

- Zmiana CSV z basename-only na path/fingerprint/ID.
- Punkt 18 z pierwotnego review: CSP, `assetProtocol`, ustawienia renderera i SmartScreen.
- Nowy redesign wizualny, zmiana położenia elementów lub nowe motywy.
- Zmiana formatu progress eventu albo kluczy persistence bez osobnej decyzji Jana.
- Tworzenie, aktualizacja i uruchamianie testów/buildów bez osobnego polecenia Jana.
