# Etap 09 - Lokalizacja i quality gates

## Metryka

- Priorytet: średni
- Status: Ukończony
- Zależności: Etapy 07 i 08
- Następny etap: 10

## Cel

Doprowadzić wszystkie locale i narzędzia generujące do sprawdzalnej spójności oraz dodać powtarzalne bramki jakości dla konfiguracji, TypeScript, Rust i zależności.

## Potwierdzone ryzyka

- Francuskiemu brakuje 83 ścieżek tłumaczeń względem angielskiego, a czeskiemu 85.
- UI używa `common.confirm`, którego nie ma w locale; istnieje tylko `lightbox.deleteConfirm.confirm`.
- Generator oczekuje nieaktualnego zakończenia `} as const;`.
- Generator próbuje wykonać obiekt zawierający identyfikatory importów poza jego zakresem i pomija język czeski.
- Placeholdery nie mają automatycznej kontroli zgodności.
- Brak workflow CI, lint, format, osobnego typecheck i coverage contract.
- `vitest.config.ts` nie jest objęty TypeScript project check.
- Bun i Rust nie są przypięte; skrypty Cargo nie używają `--locked`.
- Windows sidecary nie mają zapisanej proweniencji i checksum.

## Zadania

- [x] Ustalić `en.json` jako jednoznaczne źródło kształtu locale.
- [x] Uzupełnić wszystkie brakujące klucze, zaczynając od `common.confirm`.
- [x] Poprawić błędnie zagnieżdżone klucze czeskie.
- [x] Walidować kompletność wszystkich wartości `AppLanguage`.
- [x] Walidować zgodność placeholderów i typów wartości.
- [x] Przepisać generator tak, aby czytał JSON bez `Function`/eval.
- [x] Uwzględnić czeski i nie nadpisywać ręcznych tłumaczeń bez jawnej opcji.
- [x] Dodać skrypt walidacji locale działający bez sieci.
- [x] Objąć `vite.config.ts` i `vitest.config.ts` kontrolą TypeScript.
- [x] Dodać jawne skrypty `typecheck`, `lint`, `format:check` i kontrole Rust.
- [x] Dodać workflow CI używający lockfile i bezpiecznych profili.
- [x] Przypiąć wspieraną wersję Bun/Node i Rust albo jawnie udokumentować politykę aktualizacji.
- [x] Użyć `--locked`/frozen install w bramkach weryfikacyjnych.
- [x] Udokumentować źródło, wersję i checksum Windows ffmpeg/ffprobe.
- [x] Rozstrzygnąć nazwę produktu i poprawić `Image Viewr 3000` bez zmiany identifiera danych.

## Kryteria odbioru

- Każdy język ma komplet wymaganych kluczy i zgodne placeholdery.
- Generator działa na aktualnym formacie bez wykonywania kodu z plików tłumaczeń.
- TypeScript sprawdza kod aplikacji oraz obie konfiguracje Vite/Vitest.
- CI waliduje frontend, backend i konfigurację z użyciem lockfile.
- Toolchain i sidecary mają powtarzalną, udokumentowaną politykę wersji.

## Planowana weryfikacja

- Offline locale validation i placeholder comparison.
- Typecheck konfiguracji oraz statyczna walidacja JSON/TOML.
- CI/test/build dopiero po osobnym poleceniu zgodnym z głównym planem.

## Dziennik

### 2026-08-22 - sesja implementacyjna

- Status po sesji: Ukończony.
- Zmienione pliki: wszystkie `src/i18n/locales/*.json`; rejestr i testy i18n; `scripts/locale-*.mjs` i generator; konfiguracje TypeScript/Biome/package; workflow CI; przypięcia Node/Bun/Rust; skrypty i źródła Rust objęte bazowym rustfmt/Clippy; konfiguracja buildów Tauri/Docker; metadane sidecarów; dokumentacja rozwoju i planu.
- Wdrożone zachowanie: 11 locale ma 336 zgodnych liści; offline gate porównuje ścieżki, typy i pełne multisety placeholderów; generator czyta JSON, obejmuje wszystkie języki i domyślnie zachowuje ręczne wartości; TypeScript sprawdza aplikację oraz konfiguracje Vite/Vitest; CI używa frozen/locked resolution i nie uruchamia profilu produkcyjnego; Cargo build/test używa `--locked`.
- Decyzje i odchylenia: nazwa produktu to `Image Viewer 3000`, natomiast nazwa wewnętrzna MediaTagger i identyfikatory `com.example.mediatagger*` pozostają bez zmian; formatowanie Biome obejmuje skrypty jakości i konfiguracje, a lint cały kod JS/TS; pełny baseline rustfmt był konieczny, aby kontrola Rust nie była pozorna.
- Migracje i kompatybilność: brak migracji danych i brak zmiany identifiera; Node `22.22.0`, Bun `1.4.0` i Rust `1.98.0` są wersjami CI/release, a zakres `engines.node` pozostaje deklaracją kompatybilności.
- Utworzone lub zmienione testy: trzy testy kontraktu/generatora; test pełnej listy 11 języków; test `common.confirm` w obu destrukcyjnych dialogach ustawień; istniejące testy Rust dostosowane wyłącznie przez rustfmt/Clippy.
- Uruchomione sprawdzenia: `bun install --frozen-lockfile` bez zmian; `bun run quality` zielone (11 locale, 4/4 testy narzędzi, dwa przebiegi TypeScript, lint 193 plików, format 6 plików, rustfmt, locked Clippy); generator domyślny wykonał 0 tłumaczeń i 0 zmian; obie checksumy sidecarów zgodne; `git diff --check` bez błędów; pełne wymagane `test:all` po poprawieniu przekazania `--locked` do Tauri: Vitest 349/349 w 61 plikach, Rust 126 + 1 + 5, desktop E2E 13/13 w 2 plikach spec.
- Niewykonane sprawdzenia i ryzyka: workflow nie został uruchomiony na GitHubie; nie wykonano pełnego Docker release build ani zainstalowanego testu MSI/NSIS bez FFmpeg w `PATH`; nie przejrzano ręcznie wszystkich ekranów w 11 językach, a tłumaczenia francuskie i czeskie wymagają dalszej korekty językowej przez native speakera.
- Następny dokładny krok: rozpocząć Etap 10 - Weryfikacja końcowa.
