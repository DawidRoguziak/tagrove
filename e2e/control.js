#!/usr/bin/env node
import { spawn } from "node:child_process";
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
  driverPort,
  liveGroupMembers,
  nativePort,
  removeOwnedScratch,
  requireFreePort,
  stopOwnedGroup,
  validateRequest,
  validateSessionId
} from "./control-support.js";
import { copyPngFixtures, createPlayableFixtures } from "./fixtures.js";

const evidenceRoot = path.join(projectRoot, "artifacts", "app-control");
const help = `Usage: bun run app:control start
Keep start running in a terminal or an exec session. It prints READY with a session ID.
In another command: bun run app:control <command> <session> [arguments]
  doctor                    Check process ownership, profile, roots, and UI
  inspect [selector]        Read visible text, values, and element attributes
  click <selector>          Click a visible element
  fill <selector> <text>    Replace an input value
  keys <key> [key...]       Press keys, e.g. Enter or Escape
  scroll <selector>         Scroll an element into view
  screenshot                Capture the private display to a PNG
  logs                      Read recent process and frontend logs
  refresh                   Reload, reinstall console capture, record the gap
  diagnose                  Emit known console/error markers to test capture
  stop                      Stop owned processes; preserve evidence
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
    socket.setTimeout(120000, () => socket.destroy(new Error("Control command timed out")));
    socket.once("error", reject);
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
  let queue = Promise.resolve();
  let sequence = 0;
  let env;
  let driver;
  let display;
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

  async function run(command, args) {
    const child = await ownedSpawn(command, args);
    const { code, signal, error } = await child.completed;
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
    const title = await invoke("plugin:window|title", { label: "main" });
    const dataDir = await invoke("plugin:path|resolve_directory", {
      directory: BaseDirectory.AppData
    });
    if (title !== e2eWindowTitle || path.resolve(dataDir) !== manifest.appData) {
      throw new Error(`Refusing unexpected app identity/profile: ${title}, ${dataDir}`);
    }
    const roots = await invoke("list_scan_roots");
    if (
      roots.some(
        (root) => path.resolve(typeof root === "string" ? root : root.path) !== manifest.media
      )
    ) {
      throw new Error("Unexpected scan root; refusing further UI commands");
    }
    return {
      id,
      title,
      dataDir,
      roots,
      display: manifest.display,
      pid: process.pid,
      driverPid: driver.pid,
      displayPid: display.pid,
      evidence,
      ui: await browser.execute(() => ({
        ready: document.readyState,
        language: document.documentElement.lang
      }))
    };
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
    const health = await doctor();
    if (command === "doctor") return health;
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
    if (["click", "fill", "scroll"].includes(command)) {
      const element = await browser.$(args[0]);
      if (command === "scroll") await element.scrollIntoView();
      else {
        await element.waitForDisplayed({ timeout: 15000 });
        if (command === "click") await element.click();
        else await element.setValue(args[1]);
      }
    } else if (command === "keys") await browser.keys(args.length === 1 ? args[0] : args);
    else if (command === "refresh") await refresh();
    else if (command === "diagnose") {
      await browser.execute((marker) => {
        console.info(marker);
        console.error(`${marker}: console error`);
        window.dispatchEvent(new ErrorEvent("error", { message: `${marker}: window error` }));
      }, `app-control:${id}`);
    } else if (command === "screenshot") {
      const file = path.join(evidence, `screen-${String(++sequence).padStart(4, "0")}.png`);
      await run("import", ["-display", manifest.display, "-window", "root", file]);
      return { file };
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
      socket: path.join(scratch, "control.sock"),
      appData: path.join(scratch, "data", e2eAppIdentifier),
      media: path.join(scratch, "media")
    };
    await saveManifest();
    for (const dir of ["data", "config", "cache", "media"]) await fs.mkdir(path.join(scratch, dir));
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
    await run("bun", ["run", "build"]);
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
    display = await ownedSpawn(
      "Xvfb",
      [
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
    const number = await new Promise((resolve, reject) => {
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
      connectionRetryTimeout: 30000,
      capabilities: { "tauri:options": { application: e2eBinaryPath } }
    });
    await browser.setTimeout({ script: 30000, pageLoad: 30000, implicit: 0 });
    await doctor();
    await installConsole();
    await browser.execute(() => localStorage.setItem("media-tagger.language", "en"));
    await copyPngFixtures(manifest.media, "control", 4);
    await createPlayableFixtures(manifest.media, "control-playable");
    await invoke("add_scan_root", { path: manifest.media });
    await invoke("rescan_all_roots");
    await refresh();
    await browser.$('button[data-asset-index="0"]').waitForDisplayed({ timeout: 20000 });
    manifest.status = "ready";
    manifest.sessionId = browser.sessionId;
    manifest.processGroups = [display.pid, driver.pid];
    await saveManifest();
    server = net.createServer((socket) => {
      socket.setTimeout(120000, () => socket.destroy());
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
        queue = queue
          .then(async () => {
            let request;
            try {
              request = validateRequest(JSON.parse(data));
              await record("actions.jsonl", { phase: "before", ...request });
              const value = await handle(request);
              await record("actions.jsonl", { phase: "after", ...request, value });
              socket.end(`${JSON.stringify({ ok: true, value })}\n`);
            } catch (error) {
              await record("actions.jsonl", { ...request, error: String(error) });
              socket.end(`${JSON.stringify({ ok: false, error: String(error) })}\n`);
            }
          })
          .catch((error) => {
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
      queue = queue
        .then(() => (stopping ? undefined : drainConsole()))
        .catch((error) =>
          record("console.jsonl", { level: "capture-error", message: String(error) })
        );
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
