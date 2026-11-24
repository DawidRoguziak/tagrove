import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { COMMAND_CLASS, validateRequest } from "../control-support.js";
import { validateScenarioId } from "./gpui-control-support.js";
import { validateSeed } from "./seed.js";

// Comparison scenarios: one file per scenario in scenarios/<id>.js, default-exporting
//   { id, theme: "dark" | "light", seed: "empty" | "control", tauri: [[command, ...args], ...] }
// The same id starts the GPUI application in that state (`media_tagger_gpui --scenario <id>`,
// src-gpui/crates/media_tagger_client/src/scenario.rs). `tauri` holds the app:control commands
// that bring the Tauri application there, after the harness has applied `theme` and `seed`
// (tauriSteps). The catalogue with the descriptions is `scenarios` in
// docs/plans/gpui-port/epics.json.

export const scenariosDirectory = fileURLToPath(new URL("./scenarios", import.meta.url));
export const THEMES = Object.freeze(["dark", "light"]);

// A scenario step may act on the UI and wait for it. It may not manage the session or read
// evidence: the harness starts, captures and stops.
export const STEP_COMMANDS = Object.freeze(
  Object.keys(COMMAND_CLASS).filter(
    (command) => COMMAND_CLASS[command] === "act" || command === "wait" || command === "refresh"
  )
);

export function validateStep(step) {
  if (!Array.isArray(step) || !step.length || step.some((part) => typeof part !== "string"))
    throw new Error(`A scenario step is [command, ...arguments] of strings: ${JSON.stringify(step)}`);
  const [command, ...args] = step;
  if (!STEP_COMMANDS.includes(command))
    throw new Error(`Scenario steps cannot run ${JSON.stringify(command)}`);
  validateRequest({ command, args });
  return step;
}

export function validateScenario(value, expectedId) {
  if (!value || typeof value !== "object") throw new Error("A scenario is an object");
  const unknown = Object.keys(value).filter((key) => !["id", "theme", "seed", "tauri"].includes(key));
  if (unknown.length) throw new Error(`Unknown scenario fields: ${unknown.join(", ")}`);
  const id = validateScenarioId(value.id);
  if (expectedId !== undefined && id !== expectedId)
    throw new Error(`Scenario ${id} must live in scenarios/${id}.js, not ${expectedId}.js`);
  if (!THEMES.includes(value.theme))
    throw new Error(`Scenario ${id}: theme must be one of ${THEMES.join(", ")}`);
  validateSeed(value.seed);
  if (!Array.isArray(value.tauri)) throw new Error(`Scenario ${id}: tauri must be a list of steps`);
  return Object.freeze({
    id,
    theme: value.theme,
    seed: value.seed,
    tauri: Object.freeze(value.tauri.map((step) => Object.freeze([...validateStep(step)])))
  });
}

// Every scenario of the directory, by id, in id order.
export async function loadCatalog(directory = scenariosDirectory) {
  const catalog = new Map();
  const ids = (await fs.readdir(directory))
    .filter((name) => name.endsWith(".js"))
    .map((name) => name.slice(0, -".js".length))
    .sort();
  for (const id of ids) {
    const module = await import(pathToFileURL(path.join(directory, `${id}.js`)).href);
    catalog.set(id, validateScenario(module.default, id));
  }
  return catalog;
}

export function requireScenario(catalog, id) {
  validateScenarioId(id);
  const scenario = catalog.get(id);
  if (!scenario)
    throw new Error(`Unknown scenario ${id}; known: ${[...catalog.keys()].join(", ") || "none"}`);
  return scenario;
}

const SETTINGS_OPEN = [
  ["click", 'button[aria-label="Open settings"]'],
  ["wait", '[data-testid="scan-settings-section"]']
];
// Leaving the settings page restores focus to its button. The reload after it drops that and
// every other transient state, so the capture shows what a fresh start on the prepared
// profile shows, which is what a GPUI scenario start is.
const SETTINGS_CLOSE = [
  ["click", 'button[aria-label="Back"]'],
  ["wait", "#open-settings-button"],
  ["refresh"]
];
const THEME_LIGHT = [
  ["scroll", "#settings-appearance"],
  // The label, as the desktop specs do: its invisible radio input covers the visible span.
  ["click", '.theme-choice:has(input[value="light"])'],
  ["wait", 'html[data-theme="light"]']
];
// `app:control start` always indexes its own seeded root. Removing that root through the
// settings page leaves a library with no scan roots and no assets.
const LIBRARY_EMPTY = [
  ["scroll", "#settings-scan"],
  ["click", '//*[@data-testid="scan-settings-section"]//li//button[normalize-space(.)="Remove"]'],
  ["wait", '[role="dialog"][aria-labelledby="remove-scan-root-confirm-heading"]'],
  [
    "click",
    '//*[@role="dialog"][@aria-labelledby="remove-scan-root-confirm-heading"]//button[normalize-space(.)="Remove"]'
  ],
  ["wait", '[data-testid="scan-settings-section"] li', "gone"],
  ["wait", "idle"]
];

// The `duplicates` seed: the harness has added one file to the root `app:control start` indexed
// (compare.js, `addControlDuplicate`); a rescan through the settings page indexes it.
const LIBRARY_RESCAN = [
  ["scroll", "#settings-scan"],
  ["click", '//*[@data-testid="scan-settings-section"]//button[normalize-space(.)="Rescan all"]'],
  ["wait", '//*[@data-testid="scan-settings-section"]//*[starts-with(normalize-space(text()),"Done. Indexed:")]'],
  ["wait", "idle"]
];

// The app:control commands of one scenario, from the state `app:control start` leaves (English,
// dark theme, the `control` seed indexed, gallery view): theme and seed through the settings
// page (the light theme chosen, the root removed for `empty` or rescanned for `duplicates`) and
// a reload, then the scenario's own steps, then a wait until no loader is visible.
export function tauriSteps(scenario) {
  const settings = [
    ...(scenario.theme === "light" ? THEME_LIGHT : []),
    ...(scenario.seed === "empty" ? LIBRARY_EMPTY : []),
    ...(scenario.seed === "duplicates" ? LIBRARY_RESCAN : [])
  ];
  return [
    ...(settings.length ? [...SETTINGS_OPEN, ...settings, ...SETTINGS_CLOSE] : []),
    ...(scenario.seed === "empty" ? [["wait", '//h3[normalize-space(.)="No folders attached"]']] : []),
    ...scenario.tauri,
    ["wait", "idle"]
  ].map(validateStep);
}
