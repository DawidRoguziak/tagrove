import assert from "node:assert/strict";
import test from "node:test";
import {
  localeChanged,
  mergeLocaleTree,
  protectTemplateTokens,
  restoreTemplateTokens
} from "./generate-locales.mjs";
import { validateLocaleTree } from "./locale-contract.mjs";

test("locale validation rejects missing, extra, type, and placeholder mismatches", () => {
  const errors = validateLocaleTree(
    { greeting: "Hello {{name}}", count: 1, nested: { value: "ok" } },
    { greeting: "Bonjour {{user}}", count: "1", extra: "no" },
    "fr"
  );
  assert.deepEqual(errors, [
    'fr: greeting placeholders ["{{user}}"], expected ["{{name}}"]',
    "fr: count has type string, expected number",
    "fr: missing nested.value",
    "fr: extra extra"
  ]);
});

test("placeholder protection is reversible", () => {
  const input = "Processed {{count}}\nPath: {{asset.path}}";
  const protectedValue = protectTemplateTokens(input);
  assert.equal(restoreTemplateTokens(protectedValue.protectedText, protectedValue.tokens), input);
});

test("generator preserves reviewed values unless overwrite is explicit", async () => {
  const canonical = { title: "Title", nested: { missing: "Missing" } };
  const existing = { title: "Titre", nested: {} };
  const translated = [];
  const translate = async (value) => {
    translated.push(value);
    return `translated:${value}`;
  };

  assert.deepEqual(await mergeLocaleTree(canonical, existing, translate), {
    title: "Titre",
    nested: { missing: "translated:Missing" }
  });
  assert.deepEqual(translated, ["Missing"]);

  assert.deepEqual(await mergeLocaleTree(canonical, existing, translate, { overwrite: true }), {
    title: "translated:Title",
    nested: { missing: "translated:Missing" }
  });
});

test("generator writes a corrected reviewed confirmation word without new translations", () => {
  assert.equal(
    localeChanged(
      { lightbox: { deleteConfirm: { confirmWord: "Si" } } },
      { lightbox: { deleteConfirm: { confirmWord: "Sí" } } }
    ),
    true
  );
  assert.equal(
    localeChanged({ first: "one", second: "two" }, { second: "two", first: "one" }),
    false
  );
});
