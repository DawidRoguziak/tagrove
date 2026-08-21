# Etap 09 - Lokalizacja i quality gates

## Metryka

- Priorytet: średni
- Status: Oczekuje
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

- [ ] Ustalić `en.json` jako jednoznaczne źródło kształtu locale.
- [ ] Uzupełnić wszystkie brakujące klucze, zaczynając od `common.confirm`.
- [ ] Poprawić błędnie zagnieżdżone klucze czeskie.
- [ ] Walidować kompletność wszystkich wartości `AppLanguage`.
- [ ] Walidować zgodność placeholderów i typów wartości.
- [ ] Przepisać generator tak, aby czytał JSON bez `Function`/eval.
- [ ] Uwzględnić czeski i nie nadpisywać ręcznych tłumaczeń bez jawnej opcji.
- [ ] Dodać skrypt walidacji locale działający bez sieci.
- [ ] Objąć `vite.config.ts` i `vitest.config.ts` kontrolą TypeScript.
- [ ] Dodać jawne skrypty `typecheck`, `lint`, `format:check` i kontrole Rust.
- [ ] Dodać workflow CI używający lockfile i bezpiecznych profili.
- [ ] Przypiąć wspieraną wersję Bun/Node i Rust albo jawnie udokumentować politykę aktualizacji.
- [ ] Użyć `--locked`/frozen install w bramkach weryfikacyjnych.
- [ ] Udokumentować źródło, wersję i checksum Windows ffmpeg/ffprobe.
- [ ] Rozstrzygnąć nazwę produktu i poprawić `Image Viewr 3000` bez zmiany identifiera danych.

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

- Brak wpisów.
