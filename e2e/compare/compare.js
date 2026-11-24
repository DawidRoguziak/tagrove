#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import readline from "node:readline";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { projectRoot } from "../../scripts/gpui-build-e2e.mjs";
import {
  buildResult,
  controlMediaRoot,
  GPUI_IMAGE,
  OUTPUT_DIRECTORY,
  parseCompareArguments,
  parseControllerLine,
  RESULT_FILE,
  resetScenarioOutput,
  SIDE_BY_SIDE_IMAGE,
  settlesWithin,
  TAURI_IMAGE
} from "./compare-support.js";
import {
  desktopConflict,
  GPUI_CONTROL_LOCK,
  parseImageInfo,
  SCREEN
} from "./gpui-control-support.js";
import { loadCatalog, requireScenario, tauriSteps } from "./scenarios.js";
import { addControlDuplicate } from "./seed.js";

// `bun run compare:ui <scenario...>`: captures the same named state of the Tauri application
// (through app:control and the scenario's steps) and of the GPUI application (through
// gpui:control and its start state), and leaves tauri.png, gpui.png, side-by-side.png and
// result.json in artifacts/gpui-compare/<scenario>/. The two applications never run at the
// same time: every Tauri capture finishes and stops its session before the GPUI session starts.

const execFileAsync = promisify(execFile);
const tauriControl = fileURLToPath(new URL("../control.js", import.meta.url));
const gpuiControl = fileURLToPath(new URL("./gpui-control.js", import.meta.url));
// A cold build of either application can take many minutes.
const READY_TIMEOUT_MS = 45 * 60_000;
const COMMAND_TIMEOUT_MS = 200_000;
const STOP_TIMEOUT_MS = 90_000;
const signalExitCodes = { SIGHUP: 129, SIGINT: 130, SIGTERM: 143 };
const ALERT_PROBE_NOISE = /ERROR webdriver: WebDriverError: no such alert/;

// The controller whose session is live, if any; an interrupted run stops it before exiting.
let active = null;
// Set by the first signal. From then on nothing new starts: the run only stops what it started
// and waits for the exit the signal handler performs.
let interrupted = false;
const haltIfInterrupted = () => (interrupted ? new Promise(() => {}) : Promise.resolve());

// One `start` process of app:control or gpui:control and the client commands of its session.
function createController(label, script, { ipc = false } = {}) {
  let child;
  let id = null;
  let ready = false;
  let exited = null;
  let stopped = null;
  const log = (line) => console.log(`[${label}] ${line}`);

  async function command(name, ...args) {
    const { stdout } = await execFileAsync(process.execPath, [script, name, id, ...args], {
      cwd: projectRoot,
      encoding: "utf8",
      timeout: COMMAND_TIMEOUT_MS,
      maxBuffer: 16 * 1024 * 1024
    }).catch((error) => {
      const detail = String(error.stderr ?? "").trim() || error.message;
      throw new Error(`${label} ${name} ${args.join(" ")} failed: ${detail}`.trim());
    });
    return JSON.parse(stdout);
  }

  return {
    label,
    command,
    start() {
      child = spawn(process.execPath, [script, "start"], {
        cwd: projectRoot,
        stdio: ["ignore", "pipe", "pipe", ...(ipc ? ["ipc"] : [])]
      });
      exited = new Promise((resolve) => {
        child.once("exit", (code, signal) => resolve({ code, signal }));
        child.once("error", (error) => resolve({ error }));
      });
      const errors = [];
      readline.createInterface({ input: child.stderr }).on("line", (line) => {
        // app:control probes for an open alert before every command; the WebDriver client
        // logs each "no such alert" answer.
        if (ALERT_PROBE_NOISE.test(line)) return;
        errors.push(line);
        log(line);
      });
      return new Promise((resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error(`${label} did not become ready in ${READY_TIMEOUT_MS} ms`)),
          READY_TIMEOUT_MS
        );
        readline.createInterface({ input: child.stdout }).on("line", (line) => {
          const parsed = parseControllerLine(line);
          if (parsed) id = parsed.id;
          if (parsed?.type === "ready") {
            ready = true;
            clearTimeout(timer);
            log(`READY ${id}`);
            resolve(parsed.value);
          } else log(line);
        });
        void exited.then(({ code, signal, error }) => {
          clearTimeout(timer);
          reject(
            new Error(
              `${label} start ended before READY (${error ?? code ?? signal}): ${errors.slice(-5).join(" | ")}`
            )
          );
        });
      });
    },
    // Always ends the session: the stop command once the controller serves it, otherwise the
    // signal its handlers turn into the same cleanup. Only the process this run started is
    // signalled, never anything by name.
    stop() {
      stopped ??= (async () => {
        if (!child) return;
        const gone = () => child.exitCode !== null || child.signalCode !== null;
        if (!gone() && ready) {
          await command("stop").catch((error) => log(String(error)));
        }
        if (!gone() && !(await settlesWithin(exited, ready ? 5000 : 0))) child.kill("SIGTERM");
        if (!(await settlesWithin(exited, STOP_TIMEOUT_MS)))
          throw new Error(
            `${label} controller ${child.pid} did not stop within ${STOP_TIMEOUT_MS} ms; inspect its lock and session before any manual recovery.`
          );
      })();
      return stopped;
    }
  };
}

async function withController(controller, work) {
  await haltIfInterrupted();
  active = controller;
  try {
    const readyValue = await controller.start();
    return await work(readyValue);
  } finally {
    await controller.stop();
    active = null;
    await haltIfInterrupted();
  }
}

async function imageInfo(file) {
  const { stdout } = await execFileAsync("magick", ["identify", "-format", "%# %w %h", file], {
    encoding: "utf8"
  });
  return parseImageInfo(stdout);
}

async function captureTauri(scenario, directory) {
  const controller = createController("tauri", tauriControl);
  return withController(controller, async (readyValue) => {
    try {
      // The `duplicates` seed is the control seed `app:control start` indexed plus one file; the
      // scenario's first steps rescan the root through the settings page (tauriSteps).
      if (scenario.seed === "duplicates")
        await addControlDuplicate(
          controlMediaRoot(readyValue, await fs.realpath(os.tmpdir()))
        );
      for (const [name, ...args] of tauriSteps(scenario)) await controller.command(name, ...args);
    } catch (error) {
      // Keep what the window showed when a step failed; it is not the reference image.
      const failed = await controller.command("screenshot").catch(() => null);
      if (failed?.file) await fs.copyFile(failed.file, path.join(directory, "tauri-failed.png"));
      throw error;
    }
    const shot = await controller.command("screenshot");
    const target = path.join(directory, TAURI_IMAGE);
    await fs.copyFile(shot.file, target);
    return { ok: true, title: readyValue.title, ...(await imageInfo(target)) };
  });
}

async function captureGpui(scenarios, directories) {
  const captures = new Map();
  const controller = createController("gpui", gpuiControl, { ipc: true });
  await withController(controller, async () => {
    for (const scenario of scenarios) {
      try {
        const state = await controller.command("scenario", scenario.id);
        const shot = await controller.command("screenshot");
        const target = path.join(directories.get(scenario.id), GPUI_IMAGE);
        await fs.copyFile(shot.file, target);
        captures.set(scenario.id, { ok: true, title: state.title, ...(await imageInfo(target)) });
      } catch (error) {
        console.error(`[gpui] ${scenario.id}: ${error.message}`);
        captures.set(scenario.id, { ok: false, error: error.message });
      }
    }
  });
  return captures;
}

async function readDescriptions() {
  try {
    const manifest = JSON.parse(
      await fs.readFile(path.join(projectRoot, "docs/plans/gpui-port/epics.json"), "utf8")
    );
    return manifest.scenarios ?? {};
  } catch {
    return {};
  }
}

async function main() {
  const { list, scenarios: ids } = parseCompareArguments(process.argv.slice(2));
  const catalog = await loadCatalog();
  if (list) {
    for (const scenario of catalog.values())
      console.log(`${scenario.id}\ttheme=${scenario.theme}\tseed=${scenario.seed}`);
    return;
  }
  const scenarios = ids.map((id) => requireScenario(catalog, id));
  const conflict =
    desktopConflict(projectRoot) ??
    (existsSync(path.join(projectRoot, GPUI_CONTROL_LOCK))
      ? `${GPUI_CONTROL_LOCK} exists: a gpui:control session owns this checkout. Stop it first; never delete a live lock.`
      : null);
  if (conflict) throw new Error(`Refusing to compare: ${conflict}`);
  await execFileAsync("magick", ["-version"]).catch(() => {
    throw new Error("compare:ui needs ImageMagick (magick) to read and join the screenshots.");
  });

  for (const [signal, code] of Object.entries(signalExitCodes)) {
    process.once(signal, () => {
      interrupted = true;
      console.error(`compare:ui interrupted by ${signal}; stopping the running session`);
      void (active ? active.stop() : Promise.resolve())
        .catch((error) => console.error(String(error)))
        .finally(() => process.exit(code));
    });
  }

  const directories = new Map();
  for (const scenario of scenarios)
    directories.set(scenario.id, await resetScenarioOutput(projectRoot, scenario.id));

  const tauri = new Map();
  for (const scenario of scenarios) {
    console.log(`[compare] ${scenario.id}: Tauri`);
    try {
      tauri.set(scenario.id, await captureTauri(scenario, directories.get(scenario.id)));
    } catch (error) {
      console.error(`[tauri] ${scenario.id}: ${error.message}`);
      tauri.set(scenario.id, { ok: false, error: error.message });
    }
  }

  console.log(`[compare] ${ids.join(", ")}: GPUI`);
  let gpui = new Map();
  try {
    gpui = await captureGpui(scenarios, directories);
  } catch (error) {
    console.error(`[gpui] ${error.message}`);
    for (const scenario of scenarios)
      if (!gpui.has(scenario.id)) gpui.set(scenario.id, { ok: false, error: error.message });
  }

  const descriptions = await readDescriptions();
  // result.json names these directories instead of quoting them; see portableError.
  const places = [
    [projectRoot, "<checkout>"],
    [os.tmpdir(), "<tmp>"],
    [await fs.realpath(os.tmpdir()).catch(() => os.tmpdir()), "<tmp>"],
    [os.homedir(), "<home>"]
  ];
  let failed = false;
  for (const scenario of scenarios) {
    const directory = directories.get(scenario.id);
    const sides = { tauri: tauri.get(scenario.id), gpui: gpui.get(scenario.id) };
    let sideBySide = false;
    if (sides.tauri?.ok && sides.gpui?.ok) {
      await execFileAsync("magick", [
        path.join(directory, TAURI_IMAGE),
        path.join(directory, GPUI_IMAGE),
        "+append",
        path.join(directory, SIDE_BY_SIDE_IMAGE)
      ]);
      sideBySide = true;
    }
    const result = buildResult({
      scenario,
      description: descriptions[scenario.id],
      screen: SCREEN,
      ...sides,
      sideBySide,
      places
    });
    await fs.writeFile(path.join(directory, RESULT_FILE), `${JSON.stringify(result, null, 2)}\n`);
    failed ||= !result.ok;
    console.log(
      `[compare] ${scenario.id}: ${result.ok ? "ok" : "FAILED"} -> ${path.join(OUTPUT_DIRECTORY, scenario.id)}`
    );
  }
  if (failed) process.exitCode = 1;
}

main().catch((error) => {
  console.error(String(error.message ?? error));
  process.exitCode = 1;
});
