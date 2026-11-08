import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";

export const driverPort = 4446;
export const nativePort = 4447;

export function validateSessionId(id) {
  if (!/^[a-f0-9]{32}$/.test(id ?? "")) throw new Error("Expected a session ID from start.");
  return id;
}

// Every command belongs to one class. Observation cannot change application state, actions can,
// and control commands manage the session itself. Gating below uses these classes.
export const COMMAND_CLASS = Object.freeze({
  doctor: "observe",
  inspect: "observe",
  screenshot: "observe",
  logs: "observe",
  wait: "observe",
  click: "act",
  fill: "act",
  keys: "act",
  scroll: "act",
  modclick: "act",
  dblclick: "act",
  drag: "act",
  wheel: "act",
  select: "act",
  alert: "act",
  resize: "act",
  native: "act",
  stop: "control",
  refresh: "control",
  diagnose: "control"
});

export const COMMAND_ARITIES = Object.freeze({
  doctor: [0, 0],
  inspect: [0, 1],
  click: [1, 1],
  fill: [2, 2],
  keys: [1, 8],
  scroll: [1, 1],
  screenshot: [0, 0],
  logs: [0, 0],
  stop: [0, 0],
  refresh: [0, 0],
  diagnose: [0, 0],
  modclick: [2, 2],
  dblclick: [1, 1],
  drag: [2, 4],
  wheel: [2, 3],
  select: [2, 2],
  alert: [1, 2],
  wait: [1, 3],
  resize: [2, 2],
  native: [2, 6]
});

// The private display is fixed by `start` (Xvfb -screen 0 1440x900x24).
export const SCREEN = Object.freeze({ width: 1440, height: 900 });
export const MODIFIER_KEYS = Object.freeze({ Shift: "\uE008", Control: "\uE009" });
export const MODIFIER_SPECS = Object.freeze(["Shift", "Control", "Control+Shift"]);
export const WAIT_STATES = Object.freeze(["displayed", "gone", "enabled", "visible", "hidden"]);
export const WAIT_DEFAULT_MS = 15000;
export const WAIT_MAX_MS = 90000;
export const DRAG_HOLD_DEFAULT_MS = 200;
// A Linux sockaddr_un path holds 108 bytes including the terminating NUL.
export const SOCKET_PATH_LIMIT = 107;
const KEYSYM = /^[A-Za-z0-9_]+(\+[A-Za-z0-9_]+){0,3}$/;
const PRINTABLE_ASCII = /^[\x20-\x7e]+$/;

const invalid = (detail) =>
  new Error(`Invalid control command or arguments${detail ? ` (${detail})` : ""}. Run app:control help.`);

export function parseInteger(text, min, max, name) {
  if (typeof text !== "string" || !/^[+-]?\d{1,7}$/.test(text)) throw invalid(`${name} must be an integer`);
  const value = Number(text);
  if (!Number.isSafeInteger(value) || value < min || value > max)
    throw invalid(`${name} must be within ${min}..${max}`);
  return value;
}

const OFFSET = /^([+-]?\d{1,5}),([+-]?\d{1,5})$/;

// drag targets: an absolute offset "dx,dy", a selector, or "selector@dx,dy".
export function parseDragTarget(text) {
  const offset = OFFSET.exec(text);
  if (offset) {
    return {
      selector: null,
      dx: parseInteger(offset[1], -5000, 5000, "dx"),
      dy: parseInteger(offset[2], -5000, 5000, "dy")
    };
  }
  const at = text.lastIndexOf("@");
  if (at > 0) {
    const tail = OFFSET.exec(text.slice(at + 1));
    if (tail)
      return {
        selector: text.slice(0, at),
        dx: parseInteger(tail[1], -5000, 5000, "dx"),
        dy: parseInteger(tail[2], -5000, 5000, "dy")
      };
  }
  if (!text) throw invalid("empty drag target");
  return { selector: text, dx: 0, dy: 0 };
}

// Optional trailing drag arguments: [holdMs] [ctrl], in that order.
export function parseDragOptions(rest) {
  let hold;
  let ctrl = false;
  const items = [...rest];
  if (items.at(-1) === "ctrl") {
    ctrl = true;
    items.pop();
  }
  if (items.length > 1) throw invalid("drag takes [holdMs] [ctrl] after the target");
  if (items.length === 1) hold = parseInteger(items[0], 150, 3000, "holdMs");
  return { hold, ctrl };
}

export function parseModifiers(spec) {
  if (!MODIFIER_SPECS.includes(spec)) throw invalid(`modifiers must be one of ${MODIFIER_SPECS.join(", ")}`);
  return spec.split("+");
}

export function parseWait(args) {
  const [target, second, third] = args;
  if (!target) throw invalid("wait needs a selector or idle");
  let state = target === "idle" ? "idle" : "displayed";
  let timeoutText;
  if (second !== undefined) {
    if (WAIT_STATES.includes(second)) {
      if (target === "idle") throw invalid("wait idle takes only a timeout");
      state = second;
      timeoutText = third;
    } else if (third === undefined) timeoutText = second;
    else throw invalid(`wait state must be one of ${WAIT_STATES.join(", ")}`);
  }
  const timeout =
    timeoutText === undefined ? WAIT_DEFAULT_MS : parseInteger(timeoutText, 250, WAIT_MAX_MS, "timeout");
  return { target, state, timeout };
}

export function validateKeysym(text) {
  if (!KEYSYM.test(text ?? "")) throw invalid("native key expects a keysym such as Return or Control+a");
  return text;
}

export function validateNative(args) {
  const [sub, ...rest] = args;
  const x = (text) => parseInteger(text, 0, SCREEN.width - 1, "x");
  const y = (text) => parseInteger(text, 0, SCREEN.height - 1, "y");
  const exact = (count) => {
    if (rest.length !== count) throw invalid(`native ${sub} takes ${count} argument(s)`);
  };
  switch (sub) {
    case "key":
      exact(1);
      validateKeysym(rest[0]);
      break;
    case "type":
      exact(1);
      if (!PRINTABLE_ASCII.test(rest[0]) || rest[0].length > 1000)
        throw invalid("native type accepts 1..1000 printable ASCII characters");
      break;
    case "click":
      if (rest.length < 2 || rest.length > 3) throw invalid("native click <x> <y> [button 1-3]");
      x(rest[0]);
      y(rest[1]);
      if (rest[2] !== undefined) parseInteger(rest[2], 1, 3, "button");
      break;
    case "move":
      exact(2);
      x(rest[0]);
      y(rest[1]);
      break;
    case "drag":
      exact(4);
      x(rest[0]);
      y(rest[1]);
      x(rest[2]);
      y(rest[3]);
      break;
    case "wheel":
      if (rest.length < 3 || rest.length > 4) throw invalid("native wheel <x> <y> <steps> [ctrl]");
      x(rest[0]);
      y(rest[1]);
      if (parseInteger(rest[2], -50, 50, "steps") === 0) throw invalid("steps must not be 0");
      if (rest[3] !== undefined && rest[3] !== "ctrl") throw invalid("the optional flag is ctrl");
      break;
    default:
      throw invalid("native subcommand must be key, type, click, move, drag, or wheel");
  }
  return args;
}

function validateArguments(command, args) {
  switch (command) {
    case "modclick":
      parseModifiers(args[0]);
      break;
    case "drag": {
      if (!args[0]) throw invalid("drag needs a source selector");
      parseDragTarget(args[1]);
      parseDragOptions(args.slice(2));
      break;
    }
    case "wheel":
      if (parseInteger(args[1], -5000, 5000, "dy") === 0) throw invalid("dy must not be 0");
      if (args[2] !== undefined && args[2] !== "ctrl") throw invalid("the optional flag is ctrl");
      break;
    case "alert":
      if (!["accept", "dismiss", "text", "send"].includes(args[0]))
        throw invalid("alert accept|dismiss|text|send <text>");
      if ((args[0] === "send") !== (args.length === 2)) throw invalid("only alert send takes text");
      break;
    case "wait":
      parseWait(args);
      break;
    case "resize":
      parseInteger(args[0], 320, 1440, "width");
      parseInteger(args[1], 240, 900, "height");
      break;
    case "native":
      validateNative(args);
      break;
    case "select":
      if (args[1].startsWith("value=") && args[1].length === "value=".length)
        throw invalid("select value= needs a value");
      break;
  }
}

export function validateRequest(value) {
  const limits = Object.hasOwn(COMMAND_ARITIES, value?.command) ? COMMAND_ARITIES[value.command] : null;
  if (
    !limits ||
    !Array.isArray(value.args) ||
    value.args.length < limits[0] ||
    value.args.length > limits[1] ||
    value.args.some((arg) => typeof arg !== "string" || arg.length > 16000)
  ) {
    throw invalid();
  }
  validateArguments(value.command, value.args);
  return value;
}

export function commandClass(request) {
  if (request.command === "alert" && request.args[0] === "text") return "observe";
  return COMMAND_CLASS[request.command];
}

// Session states: ok, alert-open (a window.alert blocks script execution), busy-maintenance
// (an IPC identity probe timed out, e.g. during a backup export/import), tainted (a native
// command was followed by a failed doctor). Returns null when allowed or the refusal reason.
export function gateCommand(state, request) {
  const kind = commandClass(request);
  if (request.command === "stop" || request.command === "logs") return null;
  if (state === "tainted")
    return request.command === "screenshot"
      ? null
      : "Session is tainted after a native command; only logs, screenshot, and stop are allowed.";
  if (state === "alert-open")
    return ["alert", "screenshot", "doctor"].includes(request.command)
      ? null
      : "A window.alert is open; use alert text|accept|dismiss first.";
  if (state === "busy-maintenance")
    return kind === "observe"
      ? null
      : "busy-maintenance: the identity probe timed out (backup export/import or a held library lock); observe and wait, do not act.";
  return null;
}

export function isScriptTimeout(error) {
  const text = `${error?.name ?? ""} ${error?.message ?? error ?? ""}`;
  return /script timeout|timed out waiting for (?:script|async)|asynchronous script timeout/i.test(text);
}

export function isAlertOpenError(error) {
  const text = `${error?.name ?? ""} ${error?.message ?? error ?? ""}`;
  return /unexpected alert open|user prompt|alert open/i.test(text);
}

// Drag through WebDriver pointer actions: a press, a hold that satisfies the rectangle-selection
// threshold (>=150 ms), a 6 px start move (>=5 px), then the rest of the path over ~300 ms.
export function dragActions(start, end, holdMs = DRAG_HOLD_DEFAULT_MS) {
  if (!Number.isInteger(holdMs) || holdMs < 150 || holdMs > 3000) throw invalid("holdMs must be 150..3000");
  const sx = Math.round(start.x);
  const sy = Math.round(start.y);
  const ex = Math.round(end.x);
  const ey = Math.round(end.y);
  const length = Math.hypot(ex - sx, ey - sy);
  const [ux, uy] = length >= 6 ? [(ex - sx) / length, (ey - sy) / length] : [1, 0];
  const first = { x: Math.round(sx + ux * 6), y: Math.round(sy + uy * 6) };
  return [
    { type: "pointerMove", duration: 0, x: sx, y: sy },
    { type: "pointerDown", button: 0 },
    { type: "pause", duration: holdMs },
    { type: "pointerMove", duration: 50, ...first },
    { type: "pointerMove", duration: 300, x: ex, y: ey },
    { type: "pointerUp", button: 0 }
  ];
}

// Hold modifier keys around an action and always release them, even when the action throws.
export async function withModifiers(browser, modifiers, action) {
  const keys = modifiers.map((name) => {
    if (!Object.hasOwn(MODIFIER_KEYS, name)) throw invalid(`unknown modifier ${name}`);
    return MODIFIER_KEYS[name];
  });
  try {
    await browser.performActions([
      { id: "control-modifiers", type: "key", actions: keys.map((value) => ({ type: "keyDown", value })) }
    ]);
    return await action();
  } finally {
    try {
      await browser.performActions([
        {
          id: "control-modifiers",
          type: "key",
          actions: [...keys].reverse().map((value) => ({ type: "keyUp", value }))
        }
      ]);
    } finally {
      await browser.releaseActions();
    }
  }
}

// The native helper gets an environment built only from the session manifest. Nothing is
// inherited from the controller, so a stray DISPLAY or XAUTHORITY cannot redirect input.
export function nativeEnvironment(manifest) {
  validateSessionId(manifest?.id);
  if (!/^:\d+$/.test(manifest.display ?? "") || [":0", ":1"].includes(manifest.display))
    throw new Error("Refusing native input: the session display is not a private display.");
  if (typeof manifest.scratch !== "string" || !path.isAbsolute(manifest.scratch))
    throw new Error("Refusing native input: the session scratch path is not absolute.");
  return {
    PATH: "/usr/bin:/bin",
    DISPLAY: manifest.display,
    MEDIATAGGER_NATIVE_DISPLAY: manifest.display,
    XAUTHORITY: path.join(manifest.scratch, "Xauthority"),
    XDG_DATA_HOME: path.join(manifest.scratch, "data"),
    MEDIATAGGER_CONTROL_SCRATCH: manifest.scratch,
    MEDIATAGGER_CONTROL_ID: manifest.id
  };
}

export function validateSocketPath(socketPath) {
  const bytes = Buffer.byteLength(socketPath);
  if (bytes > SOCKET_PATH_LIMIT) {
    throw new Error(
      `The control socket path is ${bytes} bytes; Linux allows ${SOCKET_PATH_LIMIT}. ` +
        "Start with a shorter TMPDIR."
    );
  }
  return socketPath;
}

export async function requireFreePort(port) {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", (error) =>
      reject(
        new Error(
          error.code === "EADDRINUSE"
            ? `Port ${port} is occupied; refusing to attach.`
            : `Cannot check local port ${port}: ${error.code}. Local socket access is required.`
        )
      )
    );
    server.listen(port, "127.0.0.1", resolve);
  });
  await new Promise((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve()))
  );
}

export async function removeOwnedScratch(root, id) {
  const resolved = path.resolve(root);
  if (
    path.dirname(resolved) !== (await fs.realpath(os.tmpdir())) ||
    !path.basename(resolved).startsWith("media-tagger-control-") ||
    (await fs.lstat(resolved)).isSymbolicLink() ||
    (await fs.readFile(path.join(resolved, "owner"), "utf8")) !== validateSessionId(id)
  ) {
    throw new Error(`Refusing unsafe temporary directory cleanup: ${root}`);
  }
  await fs.rm(resolved, { recursive: true });
}

// Linux process groups let us stop native WebDriver/WebKit descendants as well as the driver.
// Exclude zombies: they cannot touch files and their reaping belongs to their parent.
export async function liveGroupMembers(group) {
  const entries = await fs.readdir("/proc");
  const members = await Promise.all(
    entries
      .filter((name) => /^\d+$/.test(name))
      .map(async (pid) => {
        try {
          const stat = await fs.readFile(`/proc/${pid}/stat`, "utf8");
          const fields = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
          return Number(fields[2]) === group && fields[0] !== "Z" ? Number(pid) : null;
        } catch (error) {
          if (error.code === "ENOENT" || error.code === "ESRCH") return null;
          throw error;
        }
      })
  );
  return members.filter((pid) => pid !== null);
}

export async function stopOwnedGroup(child) {
  if (!child.pid) return;
  for (const signal of ["SIGTERM", "SIGKILL"]) {
    if (!(await liveGroupMembers(child.pid)).length) return;
    try {
      process.kill(-child.pid, signal);
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
    for (let attempt = 0; attempt < 20; attempt++) {
      if (!(await liveGroupMembers(child.pid)).length) return;
      await delay(100);
    }
  }
  throw new Error(`Owned process group ${child.pid} did not stop; preserving temporary data.`);
}
