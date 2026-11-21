# Localization

Implementation entry points: [language registry](../../src/i18n/languages.ts), [runtime initialization](../../src/i18n/core.ts), [English resource](../../src/i18n/locales/en.json), [locale validator](../../scripts/validate-locales.mjs), [generator](../../scripts/generate-locales.mjs).

MediaTagger uses i18next with the owned Solid binding in `src/i18n/solid.tsx` for frontend localization. English is the fallback language and the canonical translation tree. Locale data is bundled with the frontend; changing language does not require a network request.

## Supported languages and sources of truth

The application supports these 11 languages, in the order shown in the settings selector:

| Code | Native selector label | Resource file | Destructive confirmation word |
| --- | --- | --- | --- |
| `en` | English | `src/i18n/locales/en.json` | `Yes` |
| `pl` | Polski | `src/i18n/locales/pl.json` | `Tak` |
| `fr` | Français | `src/i18n/locales/fr.json` | `Oui` |
| `de` | Deutsch | `src/i18n/locales/de.json` | `Ja` |
| `it` | Italiano | `src/i18n/locales/it.json` | `Sì` |
| `es` | Español | `src/i18n/locales/es.json` | `Sí` |
| `ru` | Русский | `src/i18n/locales/ru.json` | `Да` |
| `zh` | 中文（简体） | `src/i18n/locales/zh.json` | `是` |
| `ja` | 日本語 | `src/i18n/locales/ja.json` | `はい` |
| `ko` | 한국어 | `src/i18n/locales/ko.json` | `예` |
| `cs` | Čeština | `src/i18n/locales/cs.json` | `Ano` |

There is deliberately more than one source that must stay synchronized:

- `APP_LANGUAGES` in `src/i18n/languages.ts` is the ordered runtime allowlist. It controls selector order and language normalization. `AppLanguage` is derived from this tuple; application/settings consumers import it from this registry.
- `APP_LANGUAGE_NATIVE_LABELS` in the same file is the source for labels displayed by `LanguageSection`. The `settings.language.*` resource keys supply the card heading, description, and accessible selector label; they do not define its options.
- Imports and the `resources` object in `src/i18n/core.ts` register the translation tree that i18next can load for each code.
- `src/i18n/locales/<code>.json` supplies the actual resource. `en.json` defines the canonical keys, value shapes, and interpolation placeholders.

`LanguageProvider` in [`src/i18n/solid.tsx`](../../src/i18n/solid.tsx) creates one
`createLanguage` owner per provider tree. `useLanguage()` returns its normalized language
accessor, async `setLanguage` and translator `t`. The translator reads the language signal
before calling i18next, so JSX/memos recompute when the language changes. Consumers need a
provider; `useLanguage` throws when it is missing. Text kept in long-lived state must not be stored as a resolved string: store a key plus parameters (or a function of the translator) and translate where it renders, as the settings section status lines do (`docs/subsystems/settings-operations.md`). Disposal removes the provider's
`languageChanged` subscription. Core HMR cleanup removes the separate persistence listener.

`LanguageSection` builds options from `APP_LANGUAGES` and `APP_LANGUAGE_NATIVE_LABELS`.
Adding a resource alone does not make a language selectable.

Language has its own settings card and `settings-language` navigation destination directly below Appearance. The panel passes language and change callbacks from its existing `appearance` controller to `LanguageSection`.

## Initial selection, changes, and persistence

`src/i18n/core.ts` initializes i18next when the module is imported. The initial language is resolved in this order:

1. Normalize the value stored at `localStorage["media-tagger.language"]` and use it when supported.
2. Otherwise normalize `window.navigator.language` and use it when supported. `navigator.languages` is not consulted.
3. Otherwise use `en`.

Accessing `window.localStorage` or calling `getItem` may throw. Core treats either failure
as an unavailable stored preference and continues with browser language or English.

Normalization trims and lowercases the value, then returns the first `APP_LANGUAGES` code for which `normalized.startsWith(code)` is true. This accepts normal region variants such as `fr-FR`, `cs-CZ`, and `zh-CN`, but it is prefix matching rather than full BCP 47 parsing. A non-locale string beginning with a supported code can also match.

On initialization and after every i18next `languageChanged` event, the normalized code is written to both:

- `document.documentElement.lang`, for browser semantics and assistive technology;
- `localStorage["media-tagger.language"]`, for the next launch.

The storage write is best effort. Property-access errors, denied writes and quota failures
do not prevent startup or interrupt subsequent `languageChanged` listeners. The selected
language still updates i18next, the Solid binding and `<html lang>` for the current session.
Persistence across launches requires a successful write.

`changeAppLanguage` awaits `i18n.changeLanguage`. The Solid binding exposes this same async function as `setLanguage`; event handlers may intentionally discard its promise. i18next falls back to English for a key absent from the active resource.

## GPUI port

The GPUI port (`docs/plans/gpui-port/`) reads the same eleven resource files. `media_tagger_client::i18n` (`src-gpui/crates/media_tagger_client/src/i18n.rs` and `i18n/`) embeds them with `include_str!`, so `bun run locale:check` guards both interfaces, and resolves them with its own engine for the part of i18next Tagrove uses:

- dotted keys into the nested tree; a key the active language lacks falls back to English, and a key no language has shows the key itself, as i18next does;
- plural keys chosen by `count`: per language `<key>_zero` (count 0 only), then `<key>_<category>`, then `<key>`, before English is tried. The CLDR cardinal rules of the eleven languages are written by hand in `i18n/plural.rs` and pinned by tests against `Intl.PluralRules` output (every count from 0 to 120, the vector 0, 1, 2, 5, 12, 21, 22, 101, millions and fractions);
- `{{placeholder}}` interpolation without escaping; a placeholder without a value keeps its text, and inserted values are not interpolated again;
- `formattedCount` from `format_count`, which reproduces `Intl.NumberFormat(language).format` for whole counts (group separator and CLDR minimum grouping digits per language: Polish, Spanish and Italian leave four-digit counts ungrouped, `1234`, and group from five digits, with a no-break space in Polish, `12 345`, and a dot in Spanish and Italian, `12.345`);
- `translate_backend_text`, the counterpart of `backendText.ts` (`i18n:<key>`, `{"key","params"}` JSON, a known dotted key, else the text unchanged). A numeric `count` in the JSON params selects the plural.

Nesting (`$t(...)`), contexts, ordinal plurals, namespaces and format functions are not implemented; no resource uses them. A key that names a group resolves like a missing key.

`Language` mirrors `APP_LANGUAGES` and `APP_LANGUAGE_NATIVE_LABELS`; its tests parse `languages.ts` and `core.ts` and compare the set of locale files. `Language::normalize` is `normalizeLanguage` (prefix matching). The starting language is the stored preference, then the system locale in POSIX precedence (`LC_ALL`, `LC_MESSAGES`, `LANG`; the first one that is set and not empty decides, and an unsupported value such as `C` gives English rather than falling through), then English. WebKitGTK derives `navigator.language` from the same variables. The preference is stored in the profile directory, not in `localStorage` ([architecture and UI conventions](../frontend/architecture-and-ui-conventions.md#runtime-appearance-and-language-effects)).

## Translation contract

Treat `src/i18n/locales/en.json` as the canonical tree. For every other resource:

- Every English leaf key must exist at exactly the same object path, with the same value type. Extra or relocated keys are errors, even if the visible wording looks correct.
- Every translated string must contain the same multiset of i18next placeholders as English. Preserve each token exactly, including braces and identifier, for example `{{count}}`, `{{path}}`, or `{{assetId}}`. Translators may move tokens to suit grammar but must not translate, add, remove, or rename them.
- Text around a placeholder may be translated normally. HTML escaping is disabled for interpolation, so do not introduce user-authored markup into translations.
- Counts that appear in a sentence use i18next plural keys: `<key>_one`, `<key>_few`, `<key>_many` and `<key>_other`, resolved from the `count` option. Every locale carries all four leaves so the path set stays identical to English, even where the language uses fewer categories (English `few`/`many` repeat `other`; `ja`, `zh` and `ko` repeat one value). Two counts in one sentence are split into two plural keys joined in the component (`confirmDuplicateChanges` and `confirmDuplicateDeletes`); `settings.duplicates.lastResult` instead takes two pre-pluralized fragments (`lastResultGroups_*`, `lastResultAssets_*`) as its `{{groups}}`/`{{assets}}` placeholders. Where a count is a label (`Queued changes: {{count}}`) no plural is needed. Counts shown to the user are formatted with `Intl.NumberFormat` and passed as `formattedCount` (`gallery.resultCount`, `workspace.itemCount`). Plural behavior is pinned by `GalleryStatusFooter.test.tsx` and `src/features/settings/__tests__/pluralCopy.test.tsx`.
- Do not rely on English fallback to prove completeness. It prevents a missing key from appearing blank, but it also makes partial translations easy to overlook.

### Destructive confirmation words

`lightbox.deleteConfirm.confirmWord` is functional data, not merely copy. `LightboxDeleteConfirmDialog` interpolates it into `typeYesLabel` and `typeYesPlaceholder`, and enables deletion only when the input equals the word after both are trimmed, NFC-normalized and lowercased with `toLocaleLowerCase` in the active language. The match ignores case but not accents (`si` does not match `Sí`), and it deliberately avoids `localeCompare`, whose base sensitivity does not fold case in WebKitGTK. Each language therefore needs a deliberate, tested confirmation word as well as the `{{value}}` placeholder in both prompt strings. `common.yes` is unrelated to this gate.

Changing these words changes what a user must type. Verify the displayed prompt and the accepted input in the corresponding language; do not assume machine-generated wording is safe for a destructive control.

## Adding translations

### Add or change a key

1. Add or change the leaf in `src/i18n/locales/en.json` first. Choose the final object path and placeholder identifiers there.
2. Make the same structural edit in all ten non-English resource files. Translate prose, but keep the key path, leaf type, and placeholder multiset identical to English.
3. If the string is a destructive confirmation word or prompt, review the confirmation behavior described above rather than translating it mechanically.
4. Run the read-only parity check below, the focused component tests, and a build. Manually inspect the affected UI in at least English and the changed locale.

### Add a language

1. Add the code at the intended selector position in `APP_LANGUAGES`; the `AppLanguage` type is derived from this tuple.
2. Add its native-language label to `APP_LANGUAGE_NATIVE_LABELS` in `src/i18n/languages.ts`.
3. Create `src/i18n/locales/<code>.json` by copying the complete English tree, then translate every leaf while preserving placeholders and types.
4. Import the JSON and add `{ translation: <import> }` under the same code in the `resources` object in `src/i18n/core.ts`.
5. Choose and test `lightbox.deleteConfirm.confirmWord`, `typeYesLabel`, and `typeYesPlaceholder` explicitly.
6. Extend the ordered selector checks in `src/app/__tests__/shell.test.tsx` to select the new value. Add runtime coverage for normalization/persistence if the new code introduces a region or script concern.
7. Register it in the GPUI client as well: a `Language` variant in `i18n/language.rs` (code and label), its file in `locale_source` (`i18n.rs`), its plural rule in `i18n/plural.rs` and its digit grouping in `i18n/number.rs`, each pinned with vectors taken from `Intl.PluralRules` and `Intl.NumberFormat`. `cargo test --manifest-path src-gpui/Cargo.toml --locked -p media_tagger_client i18n` fails until the languages, labels and locale files agree.
8. Run parity, focused tests, a build, and the manual selector/reload checklist. Updating `scripts/generate-locales.mjs` is a separate decision and does not replace any of the preceding registration steps.

## Automated locale contract

`bun run locale:check` counts the current English leaves and treats `en.json` as canonical. It fails when any locale has:

- a missing, extra, or relocated leaf path;
- a different primitive value type; or
- a different multiset of complete i18next placeholder tokens; or
- a non-English value that contains `??` or a `?` between two letters when the English value has no `?` (a lost non-ASCII character, as in `hinzugef?gt`). Legitimate question marks at the end of a word, and any value whose English text also has a `?`, are not flagged.

The gate also rejects missing or unexpected locale JSON files, duplicate language codes, and a language list that does not start with English. It is read-only and needs no network access. `scripts/locale-tools.test.mjs` covers these failure modes and the generator's preservation policy.

Runtime tests for initial language resolution and persistence remain separate from resource parity. `src/test/setup.ts` awaits real i18next initialization and English before the test file runs. The shell tests check all ordered selector options and language changes, and the lightbox confirmation tests exercise the English `Yes` flow.

`src/app/__tests__/preferenceStorage.test.tsx` resets application modules before each
scenario so the real core evaluates under injected storage-property, read, write and
quota failures. It mounts the real App through `mountApplication`, checks browser/English
fallback, changes language/theme in Settings and checks ordinary persistence. Tauri calls
are mocked; these tests do not establish native storage-failure frequency or WebView behavior.

## Search and tag-list translations

All eleven resources provide the tag-list filter, selection summary, include/exclude
states, loading/empty/applying messages and four search validation errors. The grammar
identifiers `gN` and `tags` stay literal in translated errors. `tagButtonTitle` may remain
`{{tag}} ({{state}})` across locales because both values are interpolated; equality with
English alone is not evidence of a missing translation.

[`localization.test.tsx`](../../src/features/search/__tests__/localization.test.tsx)
renders the production tag modal and search controller/wrapper in each language. It pins
included/excluded counts, three accessible tag states, filter labels and placeholders,
and all four parser-generated alerts. It also checks that applying selections still emits
`alpha -beta`. Snapshots include the five loading/empty/applying resource messages;
those five branches are not all rendered by this test. English and Polish are controls.

These checks establish resource wording, interpolation and DOM output. They do not
establish linguistic quality, screen-reader pronunciation or layout in every locale.
Review snapshot changes as translated copy, rather than automatically accepting them.

## Locale generator

`bun run locale:generate` reads `en.json` directly and processes every non-English `AppLanguage`, including Polish and Czech. It never parses or executes `src/i18n/core.ts`. By default it translates only canonical leaves that are absent from a locale and preserves every existing value. A complete locale therefore causes no network calls and no file write.

Pass `--overwrite` only for an intentional full machine-translation draft:

```bash
bun run locale:generate -- --overwrite
```

Both modes preserve the reviewed destructive confirmation words, including `Sì` and `Sí`, and run the offline locale contract after generation. Missing strings still use the unofficial Google Translate endpoint with retries and an in-memory cache. Network output is only a draft. Review wording, accessibility labels, interpolation, layout, and destructive prompts before committing it.

The generator protects placeholders and newlines with sentinel values. The placeholder contract detects changed or missing interpolation tokens, but does not separately compare newline counts or judge translation quality. Review both in the generated diff. Do not use `--overwrite` as a routine update command.

## Safe validation

### Read-only parity check

```bash
bun run locale:check
bun run test:locale-tools
```

Run the relevant existing tests and TypeScript/Vite build after localization changes:

```bash
bun run test -- src/app/__tests__/shell.test.tsx src/features/lightbox/__tests__/LightboxModal.test.tsx
bun run build
```

### Manual checklist

1. Start from a known value by changing only `localStorage["media-tagger.language"]`; do not clear unrelated application storage. Reload and confirm that a supported stored language wins over the browser locale.
2. Remove only that key, reload, and confirm a supported browser locale or region variant is normalized. Confirm an unsupported locale falls back to English.
3. Select every language in Settings. Confirm the UI changes immediately, the selector retains the code, `document.documentElement.lang` equals that code, and the storage key contains that code.
4. Reload after selecting each changed language and confirm persistence.
5. Inspect the changed surfaces for untranslated English fallback, raw translation keys, broken interpolation, clipped text, and incorrect accessible labels.
6. Open the lightbox delete dialog in every affected language. Confirm the prompt displays the reviewed word, wrong input remains blocked, and the localized word enables confirmation.
7. Review the diff before committing. Locale work should not contain generator-driven rewrites outside the intended keys.
