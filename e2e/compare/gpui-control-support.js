import { readdirSync, readFileSync, readlinkSync, statSync } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { validateSessionId } from "../control-support.js";

// Pure checks and small owned-file helpers of `bun run gpui:control` (gpui-control.js), the
// counterpart of e2e/control-support.js for the GPUI adapter. Nothing here starts a process.

// The private display is fixed by `start`, the same screen app:control gives the Tauri window.
export const SCREEN = Object.freeze({ width: 1440, height: 900, depth: 24 });
export const SCRATCH_PREFIX = "media-tagger-gpui-control-";
// The seeded media directory of the running scenario, exported to the application.
export const SCENARIO_MEDIA_VARIABLE = "MEDIATAGGER_GPUI_SCENARIO_MEDIA";
// Driver ports of desktop E2E (wdio) and app:control. A busy one means a desktop run is live.
export const DESKTOP_E2E_PORTS = Object.freeze([4444, 4446, 4447]);
export const APP_CONTROL_LOCK = path.join("artifacts", "app-control", "active.lock");
export const GPUI_CONTROL_LOCK = path.join("artifacts", "gpui-control", "active.lock");
export const TAURI_IDENTIFIERS = Object.freeze([
  "com.example.mediatagger",
  "com.example.mediatagger.dev",
  "com.example.mediatagger.e2e"
]);
export const EXPECTED_IDENTIFIER = "com.example.mediatagger.gpui.e2e";
export const EXPECTED_WINDOW_TITLE = "Tagrove GPUI E2E";

export const COMMAND_ARITIES = Object.freeze({
  doctor: [0, 0],
  scenario: [1, 1],
  screenshot: [0, 0],
  logs: [0, 0],
  stop: [0, 0]
});

const SCENARIO_ID = /^[a-z0-9]+(-[a-z0-9]+)*$/;

const invalid = (detail) =>
  new Error(
    `Invalid control command or arguments${detail ? ` (${detail})` : ""}. Run gpui:control help.`
  );

export function validateScenarioId(id) {
  if (typeof id !== "string" || id.length > 64 || !SCENARIO_ID.test(id))
    throw invalid("a scenario id is lower-case words joined by single hyphens");
  return id;
}

export function validateRequest(value) {
  const limits = Object.hasOwn(COMMAND_ARITIES, value?.command)
    ? COMMAND_ARITIES[value.command]
    : null;
  if (
    !limits ||
    !Array.isArray(value.args) ||
    value.args.length < limits[0] ||
    value.args.length > limits[1] ||
    value.args.some((arg) => typeof arg !== "string" || arg.length > 256)
  ) {
    throw invalid();
  }
  if (value.command === "scenario") validateScenarioId(value.args[0]);
  return value;
}

// `start` takes nothing, or `--scenario <id>` to launch the application in that state at once.
export function parseStartArguments(args) {
  if (args.length === 0) return { scenario: null };
  if (args.length === 2 && args[0] === "--scenario")
    return { scenario: validateScenarioId(args[1]) };
  throw invalid("start takes no arguments or --scenario <id>");
}

// The controller launches one binary only: the isolated E2E profile of the GPUI adapter, built
// into its own target directory. These values come from scripts/gpui-build-e2e.mjs; a change
// there that points at another profile, title or target directory is refused before anything
// is built, locked or started.
export function validateGpuiIdentity({ identifier, windowTitle, targetDir, binaryPath }, root) {
  if (
    identifier !== EXPECTED_IDENTIFIER ||
    !identifier.endsWith(".gpui.e2e") ||
    TAURI_IDENTIFIERS.includes(identifier)
  ) {
    throw new Error(
      `Refusing GPUI control: expected isolated identifier ${EXPECTED_IDENTIFIER}, received ${identifier}`
    );
  }
  if (windowTitle !== EXPECTED_WINDOW_TITLE) {
    throw new Error(
      `Refusing GPUI control: expected isolated window title ${EXPECTED_WINDOW_TITLE}, received ${windowTitle}`
    );
  }
  const expectedTarget = path.resolve(root, "src-gpui", "target-e2e");
  if (
    typeof targetDir !== "string" ||
    path.resolve(targetDir) !== expectedTarget ||
    !expectedTarget.endsWith(`${path.sep}target-e2e`)
  ) {
    throw new Error(`Refusing GPUI control with unsafe target directory: ${targetDir}`);
  }
  if (
    typeof binaryPath !== "string" ||
    path.resolve(binaryPath) !== path.join(expectedTarget, "debug", "media_tagger_gpui")
  ) {
    throw new Error(`Refusing GPUI control with a binary outside the E2E target: ${binaryPath}`);
  }
}

// What `media_tagger_gpui --identifier` printed must be the E2E identifier, whatever the build
// script believes it built.
export function assertBakedIdentifier(output) {
  const baked = String(output ?? "").trim();
  if (baked !== EXPECTED_IDENTIFIER) {
    throw new Error(
      `Refusing to launch a binary built for ${JSON.stringify(baked)} instead of ${EXPECTED_IDENTIFIER}`
    );
  }
  return baked;
}

function assertPrivateDisplayName(display) {
  if (!/^:\d+$/.test(display ?? "") || [":0", ":1"].includes(display))
    throw new Error("Refusing: the session display is not a private display.");
  return display;
}

function assertScratchPath(scratch) {
  if (typeof scratch !== "string" || !path.isAbsolute(scratch))
    throw new Error("Refusing: the session scratch path is not absolute.");
  if (!path.basename(scratch).startsWith(SCRATCH_PREFIX))
    throw new Error("Refusing: the session scratch path is not a GPUI controller root.");
  return scratch;
}

// Environment of the application. XDG directories, the X authority, the D-Bus address and the
// display all point into the session, so the profile, portal state and window stay out of the
// user's session. The D-Bus address names a socket that does not exist: GPUI runs without the
// desktop portal instead of reaching the user's session bus.
export function sessionEnvironment(base, manifest) {
  const scratch = assertScratchPath(manifest?.scratch);
  const env = {
    ...base,
    XDG_DATA_HOME: path.join(scratch, "data"),
    XDG_CONFIG_HOME: path.join(scratch, "config"),
    XDG_CACHE_HOME: path.join(scratch, "cache"),
    XDG_RUNTIME_DIR: path.join(scratch, "runtime"),
    DBUS_SESSION_BUS_ADDRESS: `unix:path=${path.join(scratch, "runtime", "bus")}`,
    XAUTHORITY: path.join(scratch, "Xauthority"),
    DISPLAY: assertPrivateDisplayName(manifest.display),
    LANGUAGE: "en",
    LC_ALL: "C.UTF-8",
    LANG: "C.UTF-8",
    [SCENARIO_MEDIA_VARIABLE]: path.join(scratch, "media")
  };
  // GPUI prefers Wayland whenever a compositor answers; the session has only its Xvfb.
  delete env.WAYLAND_DISPLAY;
  delete env.WAYLAND_SOCKET;
  return env;
}

// Environment of the X tools (xprop, import). Display and authority come only from the session
// manifest, so a stray DISPLAY or XAUTHORITY of the caller cannot redirect a capture.
export function displayEnvironment(manifest, searchPath = "/usr/bin:/bin") {
  const scratch = assertScratchPath(manifest?.scratch);
  const display = assertPrivateDisplayName(manifest.display);
  return {
    PATH: searchPath,
    DISPLAY: display,
    MEDIATAGGER_NATIVE_DISPLAY: display,
    XAUTHORITY: path.join(scratch, "Xauthority")
  };
}

async function assertOwnedScratch(root, id) {
  const resolved = path.resolve(root);
  if (
    path.dirname(resolved) !== (await fs.realpath(os.tmpdir())) ||
    !path.basename(resolved).startsWith(SCRATCH_PREFIX) ||
    (await fs.lstat(resolved)).isSymbolicLink() ||
    (await fs.readFile(path.join(resolved, "owner"), "utf8")) !== validateSessionId(id)
  ) {
    throw new Error(`Refusing unsafe temporary directory cleanup: ${root}`);
  }
  return resolved;
}

export async function removeOwnedScratch(root, id) {
  await fs.rm(await assertOwnedScratch(root, id), { recursive: true });
}

// The profile directory of the GPUI E2E identifier inside the session scratch, the only app
// data the controller ever removes (the guard of clearE2eAppDataDir in e2e/wdio.conf.js).
export function profileDirectory(scratch, identifier) {
  const dataHome = path.join(path.resolve(assertScratchPath(scratch)), "data");
  const directory = path.resolve(dataHome, String(identifier));
  if (
    identifier !== EXPECTED_IDENTIFIER ||
    path.dirname(directory) !== dataHome ||
    path.basename(directory) !== EXPECTED_IDENTIFIER
  ) {
    throw new Error(`Refusing unsafe GPUI app data cleanup: ${directory}`);
  }
  return directory;
}

// A scenario starts on a fresh profile and a fresh media directory. Both live in the owned
// scratch root and nothing else is touched.
export async function resetScenarioState(scratch, id, identifier) {
  const root = await assertOwnedScratch(scratch, id);
  const profile = profileDirectory(root, identifier);
  const media = path.join(root, "media");
  await fs.rm(profile, { recursive: true, force: true });
  await fs.rm(media, { recursive: true, force: true });
  await fs.mkdir(media);
  return { profile, media };
}

// `xprop WM_NAME _NET_WM_NAME WM_CLASS _NET_WM_PID` of the application window.
export function parseWindowProperties(text) {
  const quoted = (value) => [...value.matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((match) => match[1]);
  const property = (name) => {
    const match = new RegExp(`^${name}\\([A-Z0-9_]+\\) = (.*)$`, "m").exec(text ?? "");
    return match ? match[1] : null;
  };
  const title = property("_NET_WM_NAME") ?? property("WM_NAME");
  const wmClass = property("WM_CLASS");
  const pid = property("_NET_WM_PID");
  return {
    title: title === null ? null : (quoted(title)[0] ?? null),
    wmClass: wmClass === null ? [] : quoted(wmClass),
    pid: pid !== null && /^\d+$/.test(pid.trim()) ? Number(pid.trim()) : null
  };
}

// The window on the private display must be the one the controller started: the E2E title, the
// E2E identifier as its window class, and the process the controller owns.
export function assertWindowIdentity(properties, { pid }) {
  if (properties.title !== EXPECTED_WINDOW_TITLE)
    throw new Error(`Refusing unexpected window title: ${JSON.stringify(properties.title)}`);
  if (!properties.wmClass.length || properties.wmClass.some((name) => name !== EXPECTED_IDENTIFIER))
    throw new Error(`Refusing unexpected window class: ${JSON.stringify(properties.wmClass)}`);
  if (properties.pid !== pid)
    throw new Error(`Refusing a window of process ${properties.pid}; the session owns ${pid}`);
  return properties;
}

// The launcher of `bun run test:e2e:tauri` in this checkout: a process of this user that runs
// the wdio CLI with this checkout's e2e/wdio.conf.js. It is only looked up, never signalled.
export function findDesktopE2eRunner(root, { procRoot = "/proc", uid = process.getuid?.() } = {}) {
  const config = path.resolve(root, "e2e", "wdio.conf.js");
  let entries;
  try {
    entries = readdirSync(procRoot);
  } catch {
    return null;
  }
  for (const pid of entries) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const dir = path.join(procRoot, pid);
      if (uid !== undefined && statSync(dir).uid !== uid) continue;
      const argv = readFileSync(path.join(dir, "cmdline"), "utf8").split("\0");
      if (!argv.some((argument) => ["wdio", "wdio.js"].includes(path.basename(argument)))) continue;
      const cwd = readlinkSync(path.join(dir, "cwd"));
      if (argv.some((argument) => argument && path.resolve(cwd, argument) === config))
        return Number(pid);
    } catch {
      // The process exited while scanning, or is not readable; keep looking.
    }
  }
  return null;
}

// Why a GPUI session must not start now, or null. Desktop E2E, app:control and this controller
// are display-bound and run one at a time per checkout.
export function desktopConflict(root, { exists, procRoot, uid } = {}) {
  const present =
    exists ??
    ((file) => {
      try {
        statSync(file);
        return true;
      } catch {
        return false;
      }
    });
  const appControlLock = path.join(root, APP_CONTROL_LOCK);
  if (present(appControlLock))
    return `${APP_CONTROL_LOCK} exists: an app:control session owns this checkout. Stop it first; never delete a live lock.`;
  const runner = findDesktopE2eRunner(root, { procRoot, uid });
  if (runner !== null)
    return `bun run test:e2e:tauri is running in this checkout (process ${runner}); wait until it has finished.`;
  return null;
}

export async function acquireLock(lockPath, owner) {
  let handle;
  try {
    handle = await fs.open(lockPath, "wx");
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    throw new Error(
      `A GPUI controller may already own this checkout. Inspect ${lockPath}; do not delete a live lock.`
    );
  }
  await handle.writeFile(JSON.stringify(owner));
  return handle;
}

// Removes the lock only while it still names this session, so a lock another controller wrote
// is never deleted.
export async function releaseLock(lockPath, handle, id) {
  await handle.close();
  let owner;
  try {
    owner = JSON.parse(await fs.readFile(lockPath, "utf8"));
  } catch {
    return false;
  }
  if (owner?.id !== id) return false;
  await fs.unlink(lockPath);
  return true;
}

// Two captures in a row with the same pixels, different from the display before the window
// opened, mean the window has drawn and stopped changing.
export function createSettleTracker(baseline) {
  let previous = null;
  return (signature) => {
    const settled = signature !== baseline && signature === previous;
    previous = signature;
    return settled;
  };
}

// `magick identify -format "%# %w %h"`: pixel signature, width, height.
export function parseImageInfo(text) {
  const match = /^([a-f0-9]{64}) (\d+) (\d+)$/.exec(String(text ?? "").trim());
  if (!match) throw new Error(`Unexpected image description: ${JSON.stringify(text)}`);
  return { signature: match[1], width: Number(match[2]), height: Number(match[3]) };
}
