import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { projectRoot } from "../../scripts/gpui-build-e2e.mjs";
import {
  loadCatalog,
  requireScenario,
  STEP_COMMANDS,
  tauriSteps,
  validateScenario,
  validateStep
} from "./scenarios.js";

const scenario = { id: "shell-empty", theme: "dark", seed: "empty", tauri: [] };

async function planScenarios() {
  const manifest = JSON.parse(
    await fs.readFile(path.join(projectRoot, "docs/plans/gpui-port/epics.json"), "utf8")
  );
  return manifest.scenarios;
}

// id -> theme of `SCENARIOS` in the client crate.
async function rustScenarios() {
  const source = await fs.readFile(
    path.join(projectRoot, "src-gpui/crates/media_tagger_client/src/scenario.rs"),
    "utf8"
  );
  const catalogue = source.slice(source.indexOf("pub const SCENARIOS"), source.indexOf("pub fn find"));
  return new Map(
    [...catalogue.matchAll(/id: "([^"]+)",\s*theme: ThemeName::(Dark|Light),/g)].map((match) => [
      match[1],
      match[2].toLowerCase()
    ])
  );
}

test("the first scenarios of the harness exist", async () => {
  const catalog = await loadCatalog();
  assert.deepEqual(catalog.get("shell-empty"), scenario);
  assert.deepEqual(catalog.get("shell-empty-light"), { ...scenario, id: "shell-empty-light", theme: "light" });
  assert.deepEqual([...catalog.keys()], [...catalog.keys()].sort());
});

test("every scenario is in the plan catalogue and starts the GPUI application in the same theme", async () => {
  const catalog = await loadCatalog();
  const plan = await planScenarios();
  const rust = await rustScenarios();
  assert.ok(rust.size >= 2, "the Rust catalogue was not read");
  for (const { id, theme } of catalog.values()) {
    assert.ok(Object.hasOwn(plan, id), `${id} is missing from scenarios in epics.json`);
    assert.equal(rust.get(id), theme, `${id}: theme of the GPUI start state`);
  }
  for (const id of rust.keys())
    assert.ok(catalog.has(id), `${id} has a GPUI start state and no e2e/compare/scenarios/${id}.js`);
});

test("requireScenario names the known scenarios", async () => {
  const catalog = await loadCatalog();
  assert.equal(requireScenario(catalog, "shell-empty").id, "shell-empty");
  assert.throws(() => requireScenario(catalog, "no-such"), /Unknown scenario no-such; known: .*shell-empty/);
  assert.throws(() => requireScenario(catalog, "../shell-empty"), /scenario id/);
  assert.throws(() => requireScenario(new Map(), "shell-empty"), /known: none/);
});

test("a scenario file is validated whole", () => {
  assert.deepEqual(validateScenario(scenario, "shell-empty"), scenario);
  assert.ok(Object.isFrozen(validateScenario(scenario)));
  for (const [change, message] of [
    [{ id: "Shell" }, /scenario id/],
    [{ id: "other" }, /must live in scenarios\/other\.js/],
    [{ theme: "sepia" }, /theme must be one of dark, light/],
    [{ theme: undefined }, /theme must be one of/],
    [{ seed: "personal" }, /Unknown seed/],
    [{ tauri: undefined }, /tauri must be a list/],
    [{ tauri: [["stop"]] }, /cannot run "stop"/],
    [{ gpui: [] }, /Unknown scenario fields: gpui/]
  ])
    assert.throws(() => validateScenario({ ...scenario, ...change }, "shell-empty"), message);
  assert.throws(() => validateScenario(null), /A scenario is an object/);
});

test("steps may act and wait, never manage the session", () => {
  for (const step of [
    ["click", "#open-settings-button"],
    ["wait", "idle"],
    ["wait", ".tile", "gone", "20000"],
    ["keys", "Control", "k"],
    ["fill", ".filter-input", "cat"],
    ["refresh"]
  ])
    assert.equal(validateStep(step), step);
  for (const command of ["stop", "screenshot", "doctor", "logs", "inspect", "diagnose", "start", "exec"])
    assert.throws(() => validateStep([command]), /cannot run/, command);
  for (const step of [
    [],
    "click",
    ["click"],
    ["click", 5],
    ["wait", ".tile", "sometimes"],
    ["resize", "10", "10"],
    ["native", "key", "rm -rf"]
  ])
    assert.throws(() => validateStep(step), Error, JSON.stringify(step));
  assert.ok(!STEP_COMMANDS.includes("stop") && STEP_COMMANDS.includes("click"));
});

test("an empty library is reached by removing the seeded root, then a reload", () => {
  const steps = tauriSteps(scenario);
  const commands = steps.map(([command]) => command);
  assert.deepEqual(steps[0], ["click", 'button[aria-label="Open settings"]']);
  assert.ok(steps.some(([, selector]) => selector?.includes("remove-scan-root-confirm-heading")));
  assert.ok(!steps.some(([, selector]) => selector?.includes("theme-choice")));
  assert.ok(commands.indexOf("refresh") > commands.lastIndexOf("scroll"));
  assert.deepEqual(steps.at(-2), ["wait", '//h3[normalize-space(.)="No folders attached"]']);
  assert.deepEqual(steps.at(-1), ["wait", "idle"]);
});

test("the light theme is chosen before the library is emptied, and own steps come last", () => {
  const own = [["click", "#tile-size"]];
  const steps = tauriSteps({ ...scenario, theme: "light", tauri: own });
  const theme = steps.findIndex(([, selector]) => selector?.includes("theme-choice"));
  const remove = steps.findIndex(([, selector]) => selector?.includes("scan-settings-section\"]//li"));
  assert.ok(theme > 0 && remove > theme);
  assert.deepEqual(steps.slice(-2), [...own, ["wait", "idle"]]);
});

test("the seeded dark state needs no setup", () => {
  assert.deepEqual(tauriSteps({ ...scenario, seed: "control" }), [["wait", "idle"]]);
  const light = tauriSteps({ ...scenario, seed: "control", theme: "light" });
  assert.ok(light.some(([command]) => command === "refresh"));
  assert.ok(!light.some(([, selector]) => selector?.includes("remove-scan-root")));
});

test("the duplicates seed is indexed by a rescan through the settings page, then a reload", () => {
  const own = [["click", "#tile-size"]];
  const steps = tauriSteps({ ...scenario, seed: "duplicates", tauri: own });
  const commands = steps.map(([command]) => command);
  assert.deepEqual(steps[0], ["click", 'button[aria-label="Open settings"]']);
  const rescan = steps.findIndex(([, selector]) => selector?.includes('"Rescan all"'));
  const done = steps.findIndex(([, selector]) => selector?.includes("Done. Indexed:"));
  assert.ok(rescan > 0 && done > rescan && commands.indexOf("refresh") > done);
  assert.ok(!steps.some(([, selector]) => selector?.includes("remove-scan-root")));
  assert.deepEqual(steps.slice(-2), [...own, ["wait", "idle"]]);
});

test("a scenario file must carry its own id", async (t) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "mediatagger-scenarios-test-"));
  t.after(() => fs.rm(directory, { recursive: true, force: true }));
  const body = (id) => `export default ${JSON.stringify({ ...scenario, id })};\n`;
  await fs.writeFile(path.join(directory, "b-state.mjs.txt"), "ignored");
  await fs.writeFile(path.join(directory, "b-state.js"), body("b-state"));
  await fs.writeFile(path.join(directory, "a-state.js"), body("a-state"));
  assert.deepEqual([...(await loadCatalog(directory)).keys()], ["a-state", "b-state"]);

  await fs.writeFile(path.join(directory, "c-state.js"), body("d-state"));
  await assert.rejects(loadCatalog(directory), /must live in scenarios\/d-state\.js/);
});
