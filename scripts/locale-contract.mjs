import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

export const LOCALES_DIR = "src/i18n/locales";
const LANGUAGES_PATH = "src/i18n/languages.ts";
const PLACEHOLDER_PATTERN = /\{\{\s*[\w.]+\s*\}\}/g;

export async function loadAppLanguageCodes(rootDir = process.cwd()) {
  const source = await readFile(path.join(rootDir, LANGUAGES_PATH), "utf8");
  const match = source.match(/export const APP_LANGUAGES = (\[[\s\S]*?\]) as const;/);
  if (!match) {
    throw new Error(`Cannot read APP_LANGUAGES from ${LANGUAGES_PATH}`);
  }

  const codes = JSON.parse(match[1]);
  if (
    !Array.isArray(codes) ||
    codes.length === 0 ||
    codes.some((code) => typeof code !== "string") ||
    new Set(codes).size !== codes.length ||
    codes[0] !== "en"
  ) {
    throw new Error("APP_LANGUAGES must be a non-empty, unique string array starting with en");
  }
  return codes;
}

export async function readLocale(code, rootDir = process.cwd()) {
  const localePath = path.join(rootDir, LOCALES_DIR, `${code}.json`);
  return JSON.parse(await readFile(localePath, "utf8"));
}

function nodeType(value) {
  if (Array.isArray(value)) return "array";
  if (value === null) return "null";
  return typeof value;
}

function flatten(value, currentPath = "", output = new Map()) {
  const type = nodeType(value);
  if (type === "array") {
    value.forEach((entry, index) => {
      flatten(entry, `${currentPath}[${index}]`, output);
    });
  } else if (type === "object") {
    for (const [key, entry] of Object.entries(value)) {
      flatten(entry, currentPath ? `${currentPath}.${key}` : key, output);
    }
  } else {
    output.set(currentPath, { type, value });
  }
  return output;
}

function placeholders(value) {
  return typeof value === "string"
    ? [...value.matchAll(PLACEHOLDER_PATTERN)].map(([token]) => token).sort()
    : [];
}

export function validateLocaleTree(canonical, locale, code) {
  const errors = [];
  const expected = flatten(canonical);
  const actual = flatten(locale);

  for (const [key, expectedLeaf] of expected) {
    const actualLeaf = actual.get(key);
    if (!actualLeaf) {
      errors.push(`${code}: missing ${key}`);
      continue;
    }
    if (actualLeaf.type !== expectedLeaf.type) {
      errors.push(`${code}: ${key} has type ${actualLeaf.type}, expected ${expectedLeaf.type}`);
      continue;
    }
    const expectedPlaceholders = placeholders(expectedLeaf.value);
    const actualPlaceholders = placeholders(actualLeaf.value);
    if (JSON.stringify(actualPlaceholders) !== JSON.stringify(expectedPlaceholders)) {
      errors.push(
        `${code}: ${key} placeholders ${JSON.stringify(actualPlaceholders)}, expected ${JSON.stringify(expectedPlaceholders)}`
      );
    }
  }

  for (const key of actual.keys()) {
    if (!expected.has(key)) errors.push(`${code}: extra ${key}`);
  }
  return errors;
}

export async function validateAllLocales(rootDir = process.cwd()) {
  const codes = await loadAppLanguageCodes(rootDir);
  const localeDir = path.join(rootDir, LOCALES_DIR);
  const files = (await readdir(localeDir)).filter((file) => file.endsWith(".json")).sort();
  const expectedFiles = codes.map((code) => `${code}.json`).sort();
  const errors = [];

  if (JSON.stringify(files) !== JSON.stringify(expectedFiles)) {
    errors.push(`locale files ${JSON.stringify(files)}, expected ${JSON.stringify(expectedFiles)}`);
  }

  const canonical = await readLocale("en", rootDir);
  for (const code of codes) {
    const locale = await readLocale(code, rootDir);
    errors.push(...validateLocaleTree(canonical, locale, code));
  }
  return { codes, errors };
}
