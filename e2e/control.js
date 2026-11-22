#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { randomBytes } from "node:crypto";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { BaseDirectory } from "@tauri-apps/api/path";
import { remote } from "webdriverio";
import {
  e2eAppIdentifier,
  e2eBinaryPath,
  e2eWindowTitle,
  projectRoot,
  validateE2eBuildConfiguration
} from "./build-e2e.js";
import {
  chooseDisplayNumber,
  commandClass,
  createCommandQueue,
  dragActions,
  driverPort,
  gateCommand,
  liveGroupMembers,
  NATIVE_HELPER_TIMEOUT_MS,
  nativeEnvironment,
  nativePort,
  parseDragOptions,
  parseDragTarget,
  parseInteger,
  parseModifiers,
  parseWait,
  probeFailureState,
  REQUEST_TIMEOUT_MS,
  removeOwnedScratch,
  requireFreePort,
  resolveSelector,
  SCRIPT_TIMEOUT_MS,
  SOCKET_TIMEOUT_MS,
  stopOwnedGroup,
  validateRequest,
  validateSessionId,
  validateSocketPath,
  withModifiers
} from "./control-support.js";
import { pinControlSeedOrder } from "./compare/seed.js";
import { copyPngFixtures, createPlayableFixtures } from "./fixtures.js";

const evidenceRoot = path.join(projectRoot, "artifacts", "app-control");
const nativeHelper = path.join(projectRoot, "e2e", "control-native.py");
const help = `Usage: bun run app:control start
Keep start running in a terminal or an exec session. It prints READY with a session ID.
In another command: bun run app:control <command> <session> [arguments]
Observe:
  doctor                    Check process ownership, profile, roots, UI; report the state
  inspect [selector]        Read visible text, values, and element attributes
  screenshot                Capture the private display to a PNG
  logs                      Read recent process and frontend logs
  wait <selector|idle> [displayed|gone|enabled|visible|hidden] [timeoutMs<=90000]
                            Poll every 250 ms (default displayed, 15000 ms); visible/hidden
                            use computed visibility and opacity; idle = no visible loaders
Act:
  click <selector>          Click a visible element
  dblclick <selector>       Double-click a visible element
  modclick <Shift|Control|Control+Shift> <selector>
                            Click while holding modifiers; they are always released
  fill <selector> <text>    Replace an input value
  select <selector> <text|value=...>
                            Choose a <select> option by visible text or value
  keys <key> [key...]       Press keys, e.g. Enter or Escape; several keys form a chord
  scroll <selector>         Scroll an element into view
  drag <from> <to|dx,dy|selector@dx,dy> [holdMs 150-3000] [ctrl]
                            Press, hold (default 200 ms), move 6 px, finish over ~300 ms;
                            ctrl holds Control (additive rectangle selection)
  wheel <selector> <dy +-5000> [ctrl]
                            Wheel over the element's center, optionally with Control
  alert accept|dismiss|text|send <text>
                            Handle a window.alert/confirm/prompt (text only observes)
  resize <width 320-1440> <height 240-900>
                            Resize the WebView window (no window manager on Xvfb)
  native key <keysym[+keysym]>|type <ascii>|click <x> <y> [button]|move <x> <y>
         |drag <x1> <y1> <x2> <y2>|wheel <x> <y> <steps> [ctrl]
                            XTest input on the private display (GTK video controls,
                            native dialogs); a failed doctor afterwards taints the session
Control:
  refresh                   Reload, reinstall console capture, record the gap
  diagnose                  Emit known console/error markers to test capture
  stop                      Stop owned processes; preserve evidence
Doctor states: ok, alert-open (only alert/screenshot/doctor/logs/stop), busy-maintenance
(the library probe timed out, e.g. backup export/import; observe only), unresponsive (the
window identity probe timed out, e.g. a native modal dialog; observe, native key Escape),
tainted (logs/screenshot/stop). stop runs at once, even while another command hangs.
Selectors use WebdriverIO syntax. Quote selectors and text in your shell.
Only one control session per checkout; do not build/run E2E concurrently.
No arbitrary code execution or external media-root command is exposed.`;

async function main() {
  const [command, id, ...args] = process.argv.slice(2);
  if (!command || command === "help") {
    console.log(help);
    return;
  }
  if (command === "start") {
    if (id) throw new Error("start takes no arguments");
    await start();
    return;
  }
  validateSessionId(id);
  const request = validateRequest({ command, args });
  const manifest = JSON.parse(
    await fs.readFile(path.join(evidenceRoot, id, "session.json"), "utf8")
  );
  if (manifest.status === "stopped" && command === "stop") {
    console.log(JSON.stringify({ status: "stopped", evidence: manifest.evidence }));
    return;
  }
  const result = await new Promise((resolve, reject) => {
    const socket = net.createConnection(manifest.socket);
    socket.setTimeout(SOCKET_TIMEOUT_MS, () => socket.destroy(new Error("Control command timed out")));
    socket.once("error", (error) =>
      reject(
        ["ENOENT", "ECONNREFUSED"].includes(error.code)
          ? new Error(
              `The controller for ${id} is not reachable (${error.code}); it may have exited. ` +
                `Inspect ${path.join(evidenceRoot, id, "session.json")} and the checkout lock. ` +
                "Never delete a lock because a client cannot connect."
            )
          : error
      )
    );
    socket.once("connect", () => socket.write(`${JSON.stringify(request)}\n`));
    let response = "";
    socket.on("data", (chunk) => {
      response += chunk;
    });
    socket.once("end", () => {
      try {
        resolve(JSON.parse(response));
      } catch {
        reject(new Error("Controller closed without a response"));
      }
    });
  });
  if (!result.ok) throw new Error(result.error);
  console.log(JSON.stringify(result.value, null, 2));
}

async function start() {
  validateE2eBuildConfiguration();
  await fs.mkdir(evidenceRoot, { recursive: true });
  const lockPath = path.join(evidenceRoot, "active.lock");
  const lock = await fs.open(lockPath, "wx").catch(() => {
    throw new Error(
      `A controller may already own this checkout. Inspect ${lockPath}; do not delete a live lock.`
    );
  });
  const id = randomBytes(16).toString("hex");
  await lock.writeFile(JSON.stringify({ id, pid: process.pid }));
  const evidence = path.join(evidenceRoot, id);
  const children = [];
  let scratch;
  let browser;
  let server;
  let manifest;
  let stopping;
  let monitor;
  let consolePoll;
  const commands = createCommandQueue({ isStopping: () => Boolean(stopping) });
  let sequence = 0;
  let env;
  let driver;
  let display;
  // Set when a doctor fails after native input; the session then only observes and stops.
  let tainted = false;
  let lastAlert = null;
  const saveManifest = () =>
    fs.writeFile(path.join(evidence, "session.json"), JSON.stringify(manifest, null, 2));
  const record = (file, value) =>
    fs.appendFile(
      path.join(evidence, file),
      `${JSON.stringify({ time: new Date().toISOString(), ...value })}\n`
    );

  async function ownedSpawn(command, args, options = {}) {
    const output = await fs.open(path.join(evidence, "process.log"), "a");
    const child = spawn(command, args, {
      cwd: projectRoot,
      env: env ?? process.env,
      detached: true,
      stdio: ["ignore", output.fd, output.fd],
      ...options
    });
    children.push(child);
    const started = new Promise((resolve) => {
      child.once("spawn", () => resolve(null));
      child.once("error", resolve);
    });
    child.completed = new Promise((resolve) => {
      child.once("exit", (code, signal) => resolve({ code, signal }));
      child.once("error", (error) => resolve({ error }));
    });
    await output.close();
    const error = await started;
    if (error) throw error;
    return child;
  }

  async function run(command, args, options = {}) {
    const { timeoutMs, ...spawnOptions } = options;
    const child = await ownedSpawn(command, args, spawnOptions);
    let timer;
    const limit = timeoutMs
      ? new Promise((resolve) => {
          timer = setTimeout(() => resolve({ error: `timed out after ${timeoutMs} ms` }), timeoutMs);
        })
      : null;
    const { code, signal, error } = await (limit
      ? Promise.race([child.completed, limit])
      : child.completed);
    clearTimeout(timer);
    // Retire short-lived commands now, so a long session never retains stale build/tool PIDs.
    await stopOwnedGroup(child);
    children.splice(children.indexOf(child), 1);
    if (code !== 0)
      throw new Error(
        `${command} failed (${error ?? code ?? signal}); see ${evidence}/process.log`
      );
  }

  async function invoke(command, payload = {}) {
    const result = await browser.executeAsync(
      (name, args, done) => {
        const bridge = window.__TAURI_INTERNALS__ ?? window.__TAURI__?.core;
        if (!bridge) {
          done({ ok: false, error: "Missing Tauri bridge" });
          return;
        }
        bridge.invoke(name, args).then(
          (value) => done({ ok: true, value }),
          (error) => done({ ok: false, error: String(error) })
        );
      },
      command,
      payload
    );
    if (!result.ok) throw new Error(result.error);
    return result.value;
  }

  // Returns the open alert's text or null. A driver without alert support counts as no alert.
  async function openAlertText() {
    try {
      return String(await browser.getAlertText());
    } catch {
      return null;
    }
  }

  async function doctor() {
    for (const child of [display, driver]) {
      if (
        !child ||
        child.exitCode !== null ||
        child.signalCode !== null ||
        !(await liveGroupMembers(child.pid)).length
      ) {
        throw new Error("An owned display/driver process exited");
      }
    }
    const base = {
      id,
      display: manifest.display,
      pid: process.pid,
      driverPid: driver.pid,
      displayPid: display.pid,
      evidence
    };
    // A window.alert blocks script execution; report it instead of failing every probe.
    const alert = await openAlertText();
    if (alert !== null) return { ...base, state: "alert-open", alert: alert.slice(0, 2000) };
    let title;
    let dataDir;
    let roots;
    let stage = "identity";
    try {
      title = await invoke("plugin:window|title", { label: "main" });
      dataDir = await invoke("plugin:path|resolve_directory", {
        directory: BaseDirectory.AppData
      });
      if (title !== e2eWindowTitle || path.resolve(dataDir) !== manifest.appData) {
        throw new Error(`Refusing unexpected app identity/profile: ${title}, ${dataDir}`);
      }
      stage = "library";
      roots = await invoke("list_scan_roots");
    } catch (error) {
      // The maintenance gate and the scan/thumbnail locks wait without a limit, so a library
      // probe can outlast the script timeout while a backup export/import runs. A native modal
      // dialog (GTK's own error dialog over a file chooser) can stall the identity probe, which
      // needs the app's main thread. The guards stay unchanged: the session may only observe
      // (and, when unresponsive, press Escape) until the probe answers again.
      const state = probeFailureState(error, stage);
      if (state === "alert-open") return { ...base, state };
      if (!state) throw error;
      return {
        ...base,
        ...(stage === "library" ? { title, dataDir } : {}),
        state,
        probe: stage === "library" ? "list_scan_roots" : "window identity",
        detail: String(error).slice(0, 2000),
        hint:
          state === "busy-maintenance"
            ? "A library operation holds the database; observe and wait until doctor says ok."
            : "The app does not answer; take a screenshot. A native dialog may be open: native key Escape dismisses it. stop always works."
      };
    }
    if (
      roots.some(
        (root) => path.resolve(typeof root === "string" ? root : root.path) !== manifest.media
      )
    ) {
      throw new Error("Unexpected scan root; refusing further UI commands");
    }
    return {
      ...base,
      state: tainted ? "tainted" : "ok",
      title,
      dataDir,
      roots,
      ui: await browser.execute(() => ({
        ready: document.readyState,
        language: document.documentElement.lang
      }))
    };
  }

  async function center(element) {
    return browser.execute((target) => {
      const rect = target.getBoundingClientRect();
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
    }, element);
  }

  async function displayed(selector) {
    const element = await browser.$(resolveSelector(selector));
    await element.waitForDisplayed({ timeout: 15000 });
    return element;
  }

  async function waitCondition(target, state) {
    if (state === "idle") {
      return browser.execute(() => {
        const shown = (element) => {
          if (!element.getClientRects().length) return false;
          for (let node = element; node; node = node.parentElement) {
            const style = getComputedStyle(node);
            if (style.display === "none" || Number(style.opacity) < 0.01) return false;
          }
          return getComputedStyle(element).visibility === "visible";
        };
        const flagged = (element, name) =>
          element.hasAttribute(name) && element.getAttribute(name) !== "false";
        return (
          document.readyState === "complete" &&
          ![
            ...document.querySelectorAll(
              '[data-testid="tile-thumb-loader"],[data-testid$="-section-loader"],[aria-busy="true"]'
            )
          ].some(shown) &&
          ![...document.querySelectorAll("[data-scroll-paused],[data-gallery-timeline-dragging]")].some(
            (element) =>
              flagged(element, "data-scroll-paused") ||
              flagged(element, "data-gallery-timeline-dragging")
          )
        );
      });
    }
    const element = await browser.$(resolveSelector(target));
    const exists = await element.isExisting();
    if (state === "displayed") return exists && element.isDisplayed();
    if (state === "gone") return !exists || !(await element.isDisplayed());
    if (state === "enabled") return exists && (await element.isDisplayed()) && element.isEnabled();
    if (!exists) return state === "hidden";
    const visible = await browser.execute((node) => {
      if (!node.getClientRects().length) return false;
      for (let current = node; current; current = current.parentElement) {
        const style = getComputedStyle(current);
        if (style.display === "none" || Number(style.opacity) < 0.01) return false;
      }
      return getComputedStyle(node).visibility === "visible";
    }, element);
    return state === "visible" ? visible : !visible;
  }

  async function installConsole() {
    await browser.execute(() => {
      if (window.__mediaTaggerControlConsole) return;
      const state = { entries: [], dropped: 0 };
      window.__mediaTaggerControlConsole = state;
      const stringify = (value) => {
        try {
          return typeof value === "string"
            ? value
            : value instanceof Error
              ? `${value.message}\n${value.stack}`
              : JSON.stringify(value);
        } catch {
          return String(value);
        }
      };
      const push = (level, args) => {
        try {
          if (state.entries.length >= 1000) {
            state.entries.shift();
            state.dropped++;
          }
          state.entries.push({
            time: new Date().toISOString(),
            level,
            message: args.map(stringify).join(" ").slice(0, 16000)
          });
        } catch {
          /* Observation must never interrupt application code. */
        }
      };
      for (const level of ["debug", "info", "log", "warn", "error"]) {
        const original = console[level];
        console[level] = function (...args) {
          push(level, args);
          return original.apply(this, args);
        };
      }
      window.addEventListener("error", (event) =>
        push("window.error", [event.message, event.error])
      );
      window.addEventListener("unhandledrejection", (event) =>
        push("unhandledrejection", [event.reason])
      );
      push("capture", ["Capture attached; messages before attachment are unavailable."]);
    });
  }

  async function drainConsole() {
    // While an alert is open every script call fails; record the alert once and wait.
    const alert = await openAlertText();
    if (alert !== null) {
      if (alert !== lastAlert)
        await record("console.jsonl", { level: "alert", message: alert.slice(0, 16000) });
      lastAlert = alert;
      return;
    }
    lastAlert = null;
    const captured = await browser.execute(() => {
      const state = window.__mediaTaggerControlConsole;
      if (!state) return null;
      const result = { entries: state.entries.splice(0), dropped: state.dropped };
      state.dropped = 0;
      return result;
    });
    if (!captured) {
      await record("console.jsonl", {
        level: "gap",
        message: "Document changed without capture; reinstalling."
      });
      await installConsole();
      return;
    }
    if (captured.dropped)
      await record("console.jsonl", { level: "gap", dropped: captured.dropped });
    for (const entry of captured.entries) await record("console.jsonl", entry);
  }

  async function refresh() {
    await drainConsole();
    await record("console.jsonl", {
      level: "gap",
      message: "Reload begins; capture resumes after attachment."
    });
    await browser.refresh();
    await installConsole();
    await browser.$(".filter-input").waitForDisplayed({ timeout: 20000 });
  }

  async function cleanup() {
    if (stopping) return stopping;
    stopping = (async () => {
      clearInterval(monitor);
      clearInterval(consolePoll);
      if (server) server.close();
      if (browser) {
        try {
          await Promise.race([browser.deleteSession(), delay(3000)]);
        } catch (error) {
          await record("actions.jsonl", { command: "close-session", error: String(error) });
        }
      }
      const failures = [];
      for (const child of [...children].reverse()) {
        try {
          await stopOwnedGroup(child);
        } catch (error) {
          failures.push(String(error));
        }
      }
      if (failures.length) throw new Error(failures.join("\n"));
      if (scratch) await removeOwnedScratch(scratch, id);
      if (manifest) {
        manifest.status = "stopped";
        await saveManifest();
      }
      await lock.close();
      await fs.unlink(lockPath);
    })();
    return stopping;
  }

  async function handle({ command, args }) {
    if (command === "stop") {
      await cleanup();
      return { status: "stopped", evidence };
    }
    if (command === "logs") {
      try {
        await drainConsole();
      } catch (error) {
        await record("console.jsonl", { level: "capture-error", message: String(error) });
      }
      return {
        process: (await fs.readFile(path.join(evidence, "process.log"), "utf8")).slice(-24000),
        console: (await fs.readFile(path.join(evidence, "console.jsonl"), "utf8")).slice(-24000),
        evidence
      };
    }
    const request = { command, args };
    if (tainted) {
      const refusal = gateCommand("tainted", request);
      if (refusal) throw new Error(refusal);
    }
    const health = tainted ? { state: "tainted" } : await doctor();
    if (command === "doctor") return health;
    const refusal = gateCommand(health.state, request);
    if (refusal) throw new Error(refusal);
    if (command === "screenshot") {
      const file = path.join(evidence, `screen-${String(++sequence).padStart(4, "0")}.png`);
      await run("import", ["-display", manifest.display, "-window", "root", file]);
      return { file, state: health.state };
    }
    if (command === "alert") {
      if (args[0] === "text") return { text: await browser.getAlertText() };
      if (args[0] === "accept") await browser.acceptAlert();
      else if (args[0] === "dismiss") await browser.dismissAlert();
      else await browser.sendAlertText(args[1]);
      lastAlert = null;
      await drainConsole();
      return { command, status: "ok" };
    }
    if (command === "wait") {
      const { target, state, timeout } = parseWait(args);
      const started = Date.now();
      for (;;) {
        if (await waitCondition(target, state)) {
          await drainConsole();
          return { command, status: "ok", target, state, waitedMs: Date.now() - started };
        }
        if (Date.now() - started >= timeout)
          throw new Error(`wait timed out after ${timeout} ms: ${target} ${state}`);
        await drainConsole();
        await delay(250);
      }
    }
    if (command === "inspect")
      return browser.execute((selector) => {
        const visible = (element) =>
          element.getClientRects().length && getComputedStyle(element).visibility !== "hidden";
        return {
          text: document.body.innerText.slice(0, 20000),
          elements: [
            ...document.querySelectorAll(
              selector ?? 'button,input,select,textarea,[role="dialog"],[data-testid]'
            )
          ]
            .filter(visible)
            .slice(0, 200)
            .map((element) => ({
              tag: element.tagName,
              text: element.textContent?.trim().slice(0, 300),
              value: element.value,
              attributes: Object.fromEntries(
                [...element.attributes].map((attribute) => [attribute.name, attribute.value])
              )
            }))
        };
      }, args[0]);
    if (command === "modclick") {
      const element = await displayed(args[1]);
      await withModifiers(browser, parseModifiers(args[0]), () => element.click());
    } else if (command === "dblclick") {
      await (await displayed(args[0])).doubleClick();
    } else if (command === "select") {
      const element = await displayed(args[0]);
      if (args[1].startsWith("value="))
        await element.selectByAttribute("value", args[1].slice("value=".length));
      else await element.selectByVisibleText(args[1]);
    } else if (command === "drag") {
      const source = await displayed(args[0]);
      await source.scrollIntoView({ block: "center", inline: "center" });
      const start = await center(source);
      const target = parseDragTarget(args[1]);
      let end = { x: start.x + target.dx, y: start.y + target.dy };
      if (target.selector) {
        const destination = await browser.$(resolveSelector(target.selector));
        await destination.waitForExist({ timeout: 15000 });
        const point = await center(destination);
        end = { x: point.x + target.dx, y: point.y + target.dy };
      }
      const { hold, ctrl } = parseDragOptions(args.slice(2));
      const press = () =>
        browser.performActions([
          {
            id: "control-pointer",
            type: "pointer",
            parameters: { pointerType: "mouse" },
            actions: dragActions(start, end, hold)
          }
        ]);
      // withModifiers releases keys and actions; the plain path releases actions itself.
      if (ctrl) await withModifiers(browser, ["Control"], press);
      else {
        try {
          await press();
        } finally {
          await browser.releaseActions();
        }
      }
    } else if (command === "wheel") {
      const element = await displayed(args[0]);
      const point = await center(element);
      const x = Math.round(point.x);
      const y = Math.round(point.y);
      const scroll = async () => {
        await browser.performActions([
          {
            id: "control-pointer",
            type: "pointer",
            parameters: { pointerType: "mouse" },
            actions: [{ type: "pointerMove", duration: 0, x, y }]
          }
        ]);
        await browser.performActions([
          {
            id: "control-wheel",
            type: "wheel",
            actions: [
              {
                type: "scroll",
                x,
                y,
                deltaX: 0,
                deltaY: parseInteger(args[1], -5000, 5000, "dy"),
                duration: 300
              }
            ]
          }
        ]);
      };
      if (args[2] === "ctrl") await withModifiers(browser, ["Control"], scroll);
      else {
        try {
          await scroll();
        } finally {
          await browser.releaseActions();
        }
      }
    } else if (command === "resize") {
      await browser.setWindowSize(
        parseInteger(args[0], 320, 1440, "width"),
        parseInteger(args[1], 240, 900, "height")
      );
      await drainConsole();
      return { command, status: "ok", size: await browser.getWindowSize() };
    } else if (command === "native") {
      await run("python3", [nativeHelper, ...args], {
        env: nativeEnvironment(manifest),
        timeoutMs: NATIVE_HELPER_TIMEOUT_MS
      });
      // Native input bypasses WebDriver, so prove identity again before anything else.
      let after;
      try {
        after = await doctor();
      } catch (error) {
        // A stop that killed the driver mid-doctor must not turn a stopped manifest into tainted.
        if (stopping) throw error;
        tainted = true;
        manifest.status = "tainted";
        await saveManifest();
        throw new Error(`Doctor failed after native input; session tainted: ${error}`);
      }
      if (after.state === "ok") await drainConsole();
      return { command, status: "ok", state: after.state };
    } else if (["click", "fill", "scroll"].includes(command)) {
      const element = await browser.$(resolveSelector(args[0]));
      if (command === "scroll") await element.scrollIntoView();
      else {
        await element.waitForDisplayed({ timeout: 15000 });
        if (command === "click") await element.click();
        else {
          // Select the current content so typing replaces it, even when the framework
          // restores a controlled value after a plain clear.
          await browser.execute((node) => {
            node.focus();
            if (typeof node.select === "function") node.select();
          }, element);
          if (args[1] === "") await element.clearValue();
          else await element.addValue(args[1]);
        }
      }
    } else if (command === "keys") await browser.keys(args.length === 1 ? args[0] : args);
    else if (command === "refresh") await refresh();
    else if (command === "diagnose") {
      await browser.execute((marker) => {
        console.info(marker);
        console.error(`${marker}: console error`);
        window.dispatchEvent(new ErrorEvent("error", { message: `${marker}: window error` }));
      }, `app-control:${id}`);
    }
    await drainConsole();
    return { command, status: "ok" };
  }

  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.once(signal, () => {
      void cleanup().then(
        () => process.exit(0),
        (error) => {
          console.error(error);
          process.exit(1);
        }
      );
    });
  }

  try {
    await fs.mkdir(evidence, { mode: 0o700 });
    scratch = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), "media-tagger-control-"));
    await fs.writeFile(path.join(scratch, "owner"), id, { mode: 0o600 });
    manifest = {
      id,
      pid: process.pid,
      status: "starting",
      evidence,
      scratch,
      socket: validateSocketPath(path.join(scratch, "control.sock")),
      appData: path.join(scratch, "data", e2eAppIdentifier),
      media: path.join(scratch, "media")
    };
    await saveManifest();
    for (const dir of ["data", "config", "cache", "media", "export"]) await fs.mkdir(path.join(scratch, dir));
    await requireFreePort(driverPort);
    await requireFreePort(nativePort);
    // Fail before an expensive build if a required executable is unavailable.
    for (const [command, args] of [
      ["Xvfb", ["-help"]],
      ["xauth", ["-V"]],
      ["import", ["-version"]],
      [
        process.env.TAURI_DRIVER_PATH ?? path.join(os.homedir(), ".cargo/bin/tauri-driver"),
        ["--help"]
      ],
      [process.env.WEBKIT_WEBDRIVER_PATH ?? "/usr/bin/WebKitWebDriver", ["--help"]],
      [process.env.FFMPEG_PATH ?? "ffmpeg", ["-version"]]
    ])
      await run(command, args);
    console.log(`BUILD ${id}; output: ${evidence}/process.log`);
    await run("bun", ["run", "tauri:build:e2e"]);
    // Apply isolation to runtime children, not build tools or the user's shell.
    env = {
      ...process.env,
      XDG_DATA_HOME: path.join(scratch, "data"),
      XDG_CONFIG_HOME: path.join(scratch, "config"),
      XDG_CACHE_HOME: path.join(scratch, "cache"),
      GDK_BACKEND: "x11",
      LANGUAGE: "en",
      LC_ALL: "C.UTF-8",
      LANG: "C.UTF-8",
      XAUTHORITY: path.join(scratch, "Xauthority")
    };
    delete env.WAYLAND_DISPLAY;
    const cookie = randomBytes(16).toString("hex");
    await fs.writeFile(env.XAUTHORITY, "", { mode: 0o600 });
    await run("xauth", ["-f", env.XAUTHORITY, "add", ":0", "MIT-MAGIC-COOKIE-1", cookie]);
    // An explicit high display number: the default lowest-free choice can be :0 or :1, which
    // every native check refuses. Retry with the next number if another server wins the race.
    let number;
    const tried = new Set();
    for (;;) {
      const candidate = chooseDisplayNumber(
        (n) => existsSync(`/tmp/.X11-unix/X${n}`) || existsSync(`/tmp/.X${n}-lock`),
        tried
      );
      tried.add(candidate);
      display = await ownedSpawn(
        "Xvfb",
        [
          `:${candidate}`,
          "-displayfd",
          "3",
          "-screen",
          "0",
          "1440x900x24",
          "-nolisten",
          "tcp",
          "-auth",
          env.XAUTHORITY,
          "-noreset"
        ],
        { stdio: ["ignore", "ignore", "pipe", "pipe"] }
      );
      display.stderr.on("data", (chunk) => {
        void fs.appendFile(path.join(evidence, "process.log"), chunk);
      });
      try {
        number = await new Promise((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error("Xvfb readiness timed out")), 15000);
          let data = "";
          display.stdio[3].on("data", (chunk) => {
            data += chunk;
            if (/^\d+\n$/.test(data)) {
              clearTimeout(timeout);
              resolve(data.trim());
            }
          });
          display.once("exit", () => {
            clearTimeout(timeout);
            reject(new Error("Xvfb exited before readiness"));
          });
        });
        break;
      } catch (error) {
        if (display.exitCode === null && display.signalCode === null) throw error;
        if (tried.size >= 10) throw error;
      }
    }
    manifest.display = `:${number}`;
    env.DISPLAY = manifest.display;
    await run("xauth", [
      "-f",
      env.XAUTHORITY,
      "add",
      manifest.display,
      "MIT-MAGIC-COOKIE-1",
      cookie
    ]);
    await requireFreePort(driverPort);
    await requireFreePort(nativePort);
    driver = await ownedSpawn(
      process.env.TAURI_DRIVER_PATH ?? path.join(os.homedir(), ".cargo/bin/tauri-driver"),
      [
        "--port",
        String(driverPort),
        "--native-port",
        String(nativePort),
        "--native-host",
        "127.0.0.1",
        "--native-driver",
        process.env.WEBKIT_WEBDRIVER_PATH ?? "/usr/bin/WebKitWebDriver"
      ]
    );
    for (let attempt = 0; ; attempt++) {
      if (driver.exitCode !== null) throw new Error("tauri-driver exited during startup");
      try {
        const response = await fetch(`http://127.0.0.1:${driverPort}/status`, {
          signal: AbortSignal.timeout(1000)
        });
        if (response.ok) break;
      } catch {
        /* Driver startup is asynchronous. */
      }
      if (attempt >= 50) throw new Error("Driver readiness timed out");
      await delay(200);
    }
    browser = await remote({
      hostname: "127.0.0.1",
      port: driverPort,
      logLevel: "error",
      connectionRetryCount: 0,
      connectionRetryTimeout: REQUEST_TIMEOUT_MS,
      capabilities: {
        "tauri:options": { application: e2eBinaryPath },
        // Leave window.alert open so doctor reports alert-open and `alert` can handle it.
        unhandledPromptBehavior: "ignore"
      }
    });
    await browser.setTimeout({
      script: SCRIPT_TIMEOUT_MS,
      pageLoad: SCRIPT_TIMEOUT_MS,
      implicit: 0
    });
    await doctor();
    await installConsole();
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await copyPngFixtures(manifest.media, "control", 4);
    await createPlayableFixtures(manifest.media, "control-playable");
    // The gallery order the comparison harness shares with the GPUI side (e2e/compare/seed.js).
    await pinControlSeedOrder(manifest.media);
    await invoke("add_scan_root", { path: manifest.media });
    await invoke("rescan_all_roots");
    await refresh();
    await browser.$('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
    manifest.status = "ready";
    manifest.sessionId = browser.sessionId;
    manifest.processGroups = [display.pid, driver.pid];
    await saveManifest();
    server = net.createServer((socket) => {
      socket.setTimeout(SOCKET_TIMEOUT_MS, () => socket.destroy());
      socket.on("error", () => {});
      let data = "";
      socket.on("data", (chunk) => {
        data += chunk;
        if (data.length > 128000) {
          socket.destroy();
          return;
        }
        if (!data.includes("\n")) return;
        socket.removeAllListeners("data");
        let request;
        (async () => {
          try {
            request = validateRequest(JSON.parse(data));
            // stop bypasses the queue; other commands wait for their turn and are skipped when
            // stopping began or their client left in the meantime.
            const value = await commands.dispatch(
              request,
              async () => {
                await record("actions.jsonl", {
                  phase: "before",
                  class: commandClass(request),
                  ...request
                });
                return handle(request);
              },
              () => socket.destroyed
            );
            await record("actions.jsonl", { phase: "after", ...request, value });
            socket.end(`${JSON.stringify({ ok: true, value })}\n`);
          } catch (error) {
            await record("actions.jsonl", { ...request, error: String(error) });
            socket.end(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
          }
        })().catch((error) => {
          console.error(error);
          socket.destroy();
        });
      });
    });
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(manifest.socket, resolve);
    });
    await fs.chmod(manifest.socket, 0o600);
    consolePoll = setInterval(() => {
      commands
        .poll(drainConsole)
        ?.catch((error) =>
          stopping
            ? undefined
            : record("console.jsonl", { level: "capture-error", message: String(error) })
        )
        .catch(console.error);
    }, 2000);
    monitor = setInterval(() => {
      if ([display, driver].some((child) => child.exitCode !== null || child.signalCode !== null)) {
        void cleanup().then(
          () => {
            process.exitCode = 1;
          },
          (error) => {
            console.error(error);
            process.exitCode = 1;
          }
        );
      }
    }, 1000);
    console.log(`READY ${JSON.stringify(await doctor())}`);
  } catch (error) {
    console.error(`Startup failed: ${error}`);
    if (manifest) manifest.failure = String(error);
    await record("actions.jsonl", { phase: "startup", error: String(error) }).catch(console.error);
    await cleanup();
    throw error;
  }
}

main().catch((error) => {
  console.error(String(error));
  process.exitCode = 1;
});
