import fs from "node:fs/promises";
import path from "node:path";
import { validateScenarioId } from "./gpui-control-support.js";

// Pure parts of `bun run compare:ui` (compare.js).

export const OUTPUT_DIRECTORY = path.join("artifacts", "gpui-compare");
export const TAURI_IMAGE = "tauri.png";
export const GPUI_IMAGE = "gpui.png";
export const SIDE_BY_SIDE_IMAGE = "side-by-side.png";
export const RESULT_FILE = "result.json";
export const RESULT_SCHEMA = 1;

const usage =
  "Usage: bun run compare:ui <scenario> [<scenario> ...] | bun run compare:ui --list";

// Scenario ids, each once, in the order given; or --list alone.
export function parseCompareArguments(args) {
  if (args.length === 1 && args[0] === "--list") return { list: true, scenarios: [] };
  if (!args.length) throw new Error(`compare:ui needs at least one scenario. ${usage}`);
  const scenarios = [];
  for (const argument of args) {
    if (argument.startsWith("-")) throw new Error(`Unknown option ${argument}. ${usage}`);
    validateScenarioId(argument);
    if (scenarios.includes(argument)) throw new Error(`Scenario ${argument} is named twice.`);
    scenarios.push(argument);
  }
  return { list: false, scenarios };
}

// artifacts/gpui-compare/<scenario> below the checkout, and nothing else.
export function scenarioOutputDirectory(root, id) {
  const base = path.resolve(root, OUTPUT_DIRECTORY);
  const directory = path.resolve(base, validateScenarioId(id));
  if (path.dirname(directory) !== base || path.basename(directory) !== id)
    throw new Error(`Refusing unsafe comparison output directory: ${directory}`);
  return directory;
}

// Every run starts from an empty scenario directory, so a rerun leaves the same set of files
// and never a screenshot of an earlier run next to a failed one.
export async function resetScenarioOutput(root, id) {
  const directory = scenarioOutputDirectory(root, id);
  await fs.rm(directory, { recursive: true, force: true });
  await fs.mkdir(directory, { recursive: true });
  return directory;
}

// True once `promise` settles, false after `ms`. The timer is cleared as soon as the wait is
// over, so a wait that the promise won never keeps the process alive for the rest of `ms`.
export function settlesWithin(promise, ms) {
  let timer;
  const timedOut = new Promise((resolve) => {
    timer = setTimeout(resolve, ms, false);
  });
  return Promise.race([promise.then(() => true), timedOut]).finally(() => clearTimeout(timer));
}

// The READY line of a controller (`READY {json}`) and the earlier BUILD line with the session.
export function parseControllerLine(line) {
  const ready = /^READY (\{.*\})\s*$/.exec(line);
  if (ready) {
    try {
      const value = JSON.parse(ready[1]);
      if (/^[a-f0-9]{32}$/.test(value?.id ?? "")) return { type: "ready", id: value.id, value };
    } catch {
      // Not a READY line of a controller.
    }
    return null;
  }
  const build = /^BUILD ([a-f0-9]{32});/.exec(line);
  return build ? { type: "build", id: build[1] } : null;
}

// What ends a path inside an error message.
const PATH_CHARACTER = String.raw`[^\s:;,'"()|<>\]]`;
const escapeRegExp = (text) => text.replace(/[\\^$.*+?()[\]{}|]/g, "\\$&");

// An error as result.json records it. Controller errors point at their evidence ("see
// <session directory>/process.log") and can quote any other path; here every absolute path is
// replaced: a directory in `places` (pairs of [absolute directory, placeholder], for example
// the checkout and the temporary directory) by its placeholder, so the message still says
// where to look, and any other one by <path>. The console keeps the full text.
export function portableError(message, places = []) {
  let text = String(message).replaceAll("file:///", "/");
  const known = places
    .filter(([directory]) => typeof directory === "string" && path.isAbsolute(directory))
    .map(([directory, placeholder]) => [path.resolve(directory), placeholder])
    .filter(([directory]) => directory !== path.parse(directory).root)
    .sort((a, b) => b[0].length - a[0].length);
  for (const [directory, placeholder] of known)
    text = text.replace(
      new RegExp(`(?<![\\w<>.~])${escapeRegExp(directory)}(?=/|(?!${PATH_CHARACTER}))`, "g"),
      () => placeholder
    );
  return text.replace(new RegExp(`(?<![\\w<>.~/])/(?!/)${PATH_CHARACTER}+`, "g"), "<path>");
}

function side(capture, file, screen, places) {
  if (!capture?.ok)
    return { ok: false, error: portableError(capture?.error ?? "not captured", places) };
  const result = {
    ok: true,
    file,
    width: capture.width,
    height: capture.height,
    signature: capture.signature,
    title: capture.title
  };
  if (capture.width !== screen.width || capture.height !== screen.height) {
    result.ok = false;
    result.error = `screenshot is ${capture.width}x${capture.height}, expected ${screen.width}x${screen.height}`;
  }
  return result;
}

// result.json of one scenario. It holds no time and no absolute path, so two runs that draw
// the same pixels write the same file. The error of a failed side goes through portableError
// with `places`; it keeps its session id, so that text can differ between runs.
export function buildResult({ scenario, description, screen, tauri, gpui, sideBySide, places }) {
  const tauriSide = side(tauri, TAURI_IMAGE, screen, places);
  const gpuiSide = side(gpui, GPUI_IMAGE, screen, places);
  const composed = Boolean(sideBySide) && Boolean(tauri?.ok) && Boolean(gpui?.ok);
  return {
    schema: RESULT_SCHEMA,
    scenario: scenario.id,
    description: description ?? null,
    theme: scenario.theme,
    seed: scenario.seed,
    screen: { width: screen.width, height: screen.height },
    ok: tauriSide.ok && gpuiSide.ok && composed,
    tauri: tauriSide,
    gpui: gpuiSide,
    sideBySide: composed ? SIDE_BY_SIDE_IMAGE : null
  };
}
