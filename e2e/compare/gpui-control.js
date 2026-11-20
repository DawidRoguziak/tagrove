#!/usr/bin/env node
import { execFile, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { promisify } from "node:util";
import {
  gpuiE2eBinaryPath,
  gpuiE2eIdentifier,
  gpuiE2eTargetDir,
  gpuiE2eWindowTitle,
  projectRoot,
  validateGpuiE2eBuildConfiguration
} from "../../scripts/gpui-build-e2e.mjs";
import {
  chooseDisplayNumber,
  createCommandQueue,
  liveGroupMembers,
  requireFreePort,
  SESSION_STOPPING,
  stopOwnedGroup,
  validateSessionId,
  validateSocketPath
} from "../control-support.js";
import { assertPrivateDisplay } from "../display.js";
import {
  acquireLock,
  assertBakedIdentifier,
  assertWindowIdentity,
  createSettleTracker,
  DESKTOP_E2E_PORTS,
  desktopConflict,
  displayEnvironment,
  GPUI_CONTROL_LOCK,
  parseImageInfo,
  parseStartArguments,
  parseWindowProperties,
  profileDirectory,
  releaseLock,
  removeOwnedScratch,
  resetScenarioState,
  SCRATCH_PREFIX,
  SCREEN,
  sessionEnvironment,
  validateGpuiIdentity,
  validateRequest
} from "./gpui-control-support.js";
import { loadCatalog, requireScenario } from "./scenarios.js";
import { seedMedia } from "./seed.js";

const execFileAsync = promisify(execFile);
const evidenceRoot = path.join(projectRoot, "artifacts", "gpui-control");
const lockPath = path.join(projectRoot, GPUI_CONTROL_LOCK);
const SOCKET_TIMEOUT_MS = 120_000;
const TOOL_TIMEOUT_MS = 15_000;
const WINDOW_TIMEOUT_MS = 30_000;
const SETTLE_TIMEOUT_MS = 20_000;
const SETTLE_INTERVAL_MS = 200;
const signalExitCodes = { SIGHUP: 129, SIGINT: 130, SIGTERM: 143 };
const help = `Usage: bun run gpui:control start [--scenario <id>]
Keep start running in a terminal or an exec session. It builds the GPUI E2E binary, starts it on
a private Xvfb display with scratch XDG directories and prints READY with a session ID.
In another command: bun run gpui:control <command> <session> [arguments]
  doctor            Check owned processes, the window title, class and process, and the profile
  scenario <id>     Restart the application in a named state on a fresh profile and seed
  screenshot        Capture the private display to a PNG
  logs              Read the recent process log
  stop              Stop owned processes and remove the scratch root; evidence is kept
Scenarios are the files of e2e/compare/scenarios. Only one GPUI control session per checkout,
and never next to app:control or bun run test:e2e:tauri. There is no input command: the GPUI
window is driven by its start state, not by WebDriver.`;

async function main() {
  const [command, ...rest] = process.argv.slice(2);
  if (!command || command === "help") {
    console.log(help);
    return;
  }
  if (command === "start") {
    await start(parseStartArguments(rest));
    return;
  }
  const [id, ...args] = rest;
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
    socket.setTimeout(SOCKET_TIMEOUT_MS, () =>
      socket.destroy(new Error("Control command timed out"))
    );
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

async function start({ scenario: initialScenario }) {
  validateGpuiE2eBuildConfiguration();
  validateGpuiIdentity(
    {
      identifier: gpuiE2eIdentifier,
      windowTitle: gpuiE2eWindowTitle,
      targetDir: gpuiE2eTargetDir,
      binaryPath: gpuiE2eBinaryPath
    },
    projectRoot
  );
  const catalog = await loadCatalog();
  if (initialScenario) requireScenario(catalog, initialScenario);
  const refuseConflict = async () => {
    const conflict = desktopConflict(projectRoot);
    if (conflict) throw new Error(`Refusing to start a GPUI control session: ${conflict}`);
    for (const port of DESKTOP_E2E_PORTS) {
      await requireFreePort(port).catch((error) => {
        throw new Error(
          `Refusing to start a GPUI control session: ${error.message} A desktop E2E run or an app:control session may be active.`
        );
      });
    }
  };
  await refuseConflict();
  await fs.mkdir(evidenceRoot, { recursive: true });
  const id = randomBytes(16).toString("hex");
  const lock = await acquireLock(lockPath, { id, pid: process.pid });
  const evidence = path.join(evidenceRoot, id);
  const children = [];
  let scratch;
  let server;
  let manifest;
  let stopping;
  let monitor;
  let display;
  let app;
  let appEnvironment;
  let sequence = 0;
  const commands = createCommandQueue({ isStopping: () => Boolean(stopping) });
  const saveManifest = () =>
    fs.writeFile(path.join(evidence, "session.json"), JSON.stringify(manifest, null, 2));
  const record = (value) =>
    fs.appendFile(
      path.join(evidence, "actions.jsonl"),
      `${JSON.stringify({ time: new Date().toISOString(), ...value })}\n`
    );

  // Long-lived children (Xvfb, the application) and the build run in their own process groups,
  // so stopping a group also stops what the child started. cleanup() stops the children that
  // exist when it begins, so nothing is spawned after that: it would outlive the session.
  async function ownedSpawn(command, args, options = {}) {
    const output = await fs.open(path.join(evidence, "process.log"), "a");
    if (stopping) {
      await output.close();
      throw new Error(SESSION_STOPPING);
    }
    const child = spawn(command, args, {
      cwd: projectRoot,
      env: process.env,
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
    if (error) {
      children.splice(children.indexOf(child), 1);
      throw error;
    }
    return child;
  }

  async function retire(child) {
    await stopOwnedGroup(child);
    const index = children.indexOf(child);
    if (index >= 0) children.splice(index, 1);
  }

  async function run(command, args, options = {}) {
    const child = await ownedSpawn(command, args, options);
    const { code, signal, error } = await child.completed;
    // Retire short-lived commands now, so a long session never retains stale tool PIDs.
    await retire(child);
    if (code !== 0)
      throw new Error(
        `${command} failed (${error ?? code ?? signal}); see ${evidence}/process.log`
      );
  }

  const alive = async (child) =>
    Boolean(child) &&
    child.exitCode === null &&
    child.signalCode === null &&
    (await liveGroupMembers(child.pid)).length > 0;

  // X tools get the display and authority of the session manifest only, after the same
  // private-display proof the Tauri specs use before a native capture.
  async function xTool(command, args) {
    const env = displayEnvironment(manifest, process.env.PATH);
    assertPrivateDisplay(env);
    return execFileAsync(command, args, { env, timeout: TOOL_TIMEOUT_MS, encoding: "utf8" });
  }

  async function capture(file) {
    await xTool("import", ["-display", manifest.display, "-window", "root", `png:${file}`]);
    const { stdout } = await execFileAsync("magick", ["identify", "-format", "%# %w %h", file], {
      timeout: TOOL_TIMEOUT_MS,
      encoding: "utf8"
    });
    return parseImageInfo(stdout);
  }

  async function windowProperties() {
    try {
      const { stdout } = await xTool("xprop", [
        "-display",
        manifest.display,
        "-name",
        gpuiE2eWindowTitle,
        "WM_NAME",
        "_NET_WM_NAME",
        "WM_CLASS",
        "_NET_WM_PID"
      ]);
      return parseWindowProperties(stdout);
    } catch (error) {
      // xprop exits non-zero while no window carries the title yet.
      if (typeof error.code === "number") return null;
      throw error;
    }
  }

  async function doctor({ application = true } = {}) {
    if (!(await alive(display))) throw new Error("The owned Xvfb exited");
    const base = {
      id,
      display: manifest.display,
      pid: process.pid,
      displayPid: display.pid,
      scenario: manifest.scenario,
      evidence
    };
    if (!application) return base;
    if (!(await alive(app)))
      throw new Error(
        `The application is not running; see ${evidence}/process.log. ` +
          "scenario <id> starts it again."
      );
    const properties = await windowProperties();
    if (!properties) throw new Error(`No window titled ${gpuiE2eWindowTitle} on ${manifest.display}`);
    assertWindowIdentity(properties, { pid: app.pid });
    const dataDir = profileDirectory(scratch, gpuiE2eIdentifier);
    if (!existsSync(path.join(dataDir, "instance.lock")))
      throw new Error(`The application did not open its profile in ${dataDir}`);
    return {
      ...base,
      state: "ok",
      appPid: app.pid,
      title: properties.title,
      windowClass: properties.wmClass[0],
      dataDir
    };
  }

  async function stopApplication() {
    if (!app) return;
    const child = app;
    app = undefined;
    await retire(child);
  }

  // Starts the application on a fresh profile and a freshly seeded media directory, then waits
  // until its window carries the E2E identity and the display has stopped changing.
  async function launch(scenario) {
    await stopApplication();
    await resetScenarioState(scratch, id, gpuiE2eIdentifier);
    await seedMedia(manifest.media, scenario?.seed ?? "empty");
    const settleFile = path.join(scratch, "settle.png");
    const baseline = (await capture(settleFile)).signature;
    manifest.scenario = scenario?.id ?? null;
    app = await ownedSpawn(gpuiE2eBinaryPath, scenario ? ["--scenario", scenario.id] : [], {
      env: appEnvironment
    });
    manifest.appPid = app.pid;
    await saveManifest();
    const exited = () => app.exitCode !== null || app.signalCode !== null;
    const failIfExited = () => {
      if (exited())
        throw new Error(
          `The application exited during startup (${app.exitCode ?? app.signalCode}); see ${evidence}/process.log`
        );
    };
    const windowDeadline = Date.now() + WINDOW_TIMEOUT_MS;
    for (;;) {
      failIfExited();
      const properties = await windowProperties();
      // The title is set before the window class; wait until the identity is complete.
      if (properties?.wmClass.length && properties.pid !== null) {
        assertWindowIdentity(properties, { pid: app.pid });
        break;
      }
      if (Date.now() >= windowDeadline)
        throw new Error(`No window titled ${gpuiE2eWindowTitle} within ${WINDOW_TIMEOUT_MS} ms`);
      await delay(100);
    }
    const settled = createSettleTracker(baseline);
    const settleDeadline = Date.now() + SETTLE_TIMEOUT_MS;
    for (;;) {
      failIfExited();
      if (settled((await capture(settleFile)).signature)) break;
      if (Date.now() >= settleDeadline)
        throw new Error(`The window did not settle within ${SETTLE_TIMEOUT_MS} ms`);
      await delay(SETTLE_INTERVAL_MS);
    }
    await fs.rm(settleFile, { force: true });
  }

  async function cleanup() {
    if (stopping) return stopping;
    stopping = (async () => {
      clearInterval(monitor);
      if (server) server.close();
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
      await releaseLock(lockPath, lock, id);
    })();
    return stopping;
  }

  async function handle({ command, args }) {
    if (command === "stop") {
      await cleanup();
      return { status: "stopped", evidence };
    }
    if (command === "logs")
      return {
        process: (await fs.readFile(path.join(evidence, "process.log"), "utf8")).slice(-24000),
        evidence
      };
    if (command === "scenario") {
      const scenario = requireScenario(catalog, args[0]);
      await doctor({ application: false });
      await launch(scenario);
      return { command, status: "ok", ...(await doctor()) };
    }
    const health = await doctor();
    if (command === "doctor") return health;
    // screenshot
    const file = path.join(evidence, `screen-${String(++sequence).padStart(4, "0")}.png`);
    const image = await capture(file);
    return { file, ...image, scenario: manifest.scenario, state: health.state };
  }

  const exitAfterCleanup = (code) =>
    cleanup().then(
      () => process.exit(code),
      (error) => {
        console.error(error);
        process.exit(1);
      }
    );
  // An interrupted session must not report success.
  for (const [signal, code] of Object.entries(signalExitCodes))
    process.once(signal, () => void exitAfterCleanup(code));
  // Started by compare:ui over an IPC channel: when that launcher goes away, stop instead of
  // leaving the display, the application and the lock behind.
  if (typeof process.send === "function") {
    process.once("disconnect", () => void exitAfterCleanup(1));
    process.channel?.unref();
  }

  try {
    await fs.mkdir(evidence, { mode: 0o700 });
    // The lock is ours now; a desktop run that started in between still wins.
    await refuseConflict();
    scratch = await fs.mkdtemp(path.join(await fs.realpath(os.tmpdir()), SCRATCH_PREFIX));
    await fs.writeFile(path.join(scratch, "owner"), id, { mode: 0o600 });
    manifest = {
      id,
      pid: process.pid,
      status: "starting",
      evidence,
      scratch,
      socket: validateSocketPath(path.join(scratch, "control.sock")),
      appData: profileDirectory(scratch, gpuiE2eIdentifier),
      media: path.join(scratch, "media"),
      scenario: null
    };
    await saveManifest();
    for (const dir of ["data", "config", "cache", "media"]) await fs.mkdir(path.join(scratch, dir));
    await fs.mkdir(path.join(scratch, "runtime"), { mode: 0o700 });
    // Fail before the build if a required executable is unavailable.
    for (const [command, args] of [
      ["Xvfb", ["-help"]],
      ["xauth", ["-V"]],
      ["xprop", ["-version"]],
      ["import", ["-version"]],
      ["magick", ["-version"]],
      [process.env.FFMPEG_PATH ?? "ffmpeg", ["-version"]]
    ])
      await run(command, args);
    console.log(`BUILD ${id}; output: ${evidence}/process.log`);
    await run("bun", ["run", "gpui:build:e2e"]);
    assertBakedIdentifier(
      (await execFileAsync(gpuiE2eBinaryPath, ["--identifier"], { encoding: "utf8" })).stdout
    );
    const xauthority = path.join(scratch, "Xauthority");
    const cookie = randomBytes(16).toString("hex");
    await fs.writeFile(xauthority, "", { mode: 0o600 });
    // An explicit high display number, never the lowest free one (which can be :0 or :1).
    // Retry with the next number if another server wins the race.
    let number;
    const tried = new Set();
    for (;;) {
      const candidate = chooseDisplayNumber(
        (n) => existsSync(`/tmp/.X11-unix/X${n}`) || existsSync(`/tmp/.X${n}-lock`),
        tried
      );
      tried.add(candidate);
      await run("xauth", ["-f", xauthority, "add", `:${candidate}`, "MIT-MAGIC-COOKIE-1", cookie]);
      display = await ownedSpawn(
        "Xvfb",
        [
          `:${candidate}`,
          "-displayfd",
          "3",
          "-screen",
          "0",
          `${SCREEN.width}x${SCREEN.height}x${SCREEN.depth}`,
          "-nolisten",
          "tcp",
          "-auth",
          xauthority,
          "-noreset"
        ],
        { stdio: ["ignore", "ignore", "pipe", "pipe"] }
      );
      display.stderr.on("data", (chunk) => {
        void fs.appendFile(path.join(evidence, "process.log"), chunk).catch(() => {});
      });
      const xvfb = display;
      try {
        number = await new Promise((resolve, reject) => {
          const timeout = setTimeout(() => reject(new Error("Xvfb readiness timed out")), 15000);
          let data = "";
          xvfb.stdio[3].on("data", (chunk) => {
            data += chunk;
            if (/^\d+\n$/.test(data)) {
              clearTimeout(timeout);
              resolve(data.trim());
            }
          });
          xvfb.once("exit", () => {
            clearTimeout(timeout);
            reject(new Error("Xvfb exited before readiness"));
          });
        });
        break;
      } catch (error) {
        // An Xvfb that cleanup() stopped did not lose a display number: no retry.
        if (stopping || (xvfb.exitCode === null && xvfb.signalCode === null)) throw error;
        children.splice(children.indexOf(xvfb), 1);
        if (tried.size >= 10) throw error;
      }
    }
    manifest.display = `:${number}`;
    manifest.displayPid = display.pid;
    appEnvironment = sessionEnvironment(process.env, manifest);
    await saveManifest();
    await launch(initialScenario ? requireScenario(catalog, initialScenario) : null);
    manifest.status = "ready";
    await saveManifest();
    server = net.createServer((socket) => {
      socket.setTimeout(SOCKET_TIMEOUT_MS, () => socket.destroy());
      socket.on("error", () => {});
      let data = "";
      socket.on("data", (chunk) => {
        data += chunk;
        if (data.length > 16000) {
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
                await record({ phase: "before", ...request });
                return handle(request);
              },
              () => socket.destroyed
            );
            await record({ phase: "after", ...request, value });
            socket.end(`${JSON.stringify({ ok: true, value })}\n`);
          } catch (error) {
            await record({ ...request, error: String(error) });
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
    monitor = setInterval(() => {
      if (display.exitCode !== null || display.signalCode !== null) void exitAfterCleanup(1);
    }, 1000);
    console.log(`READY ${JSON.stringify(await doctor())}`);
  } catch (error) {
    console.error(`Startup failed: ${error}`);
    if (manifest) manifest.failure = String(error);
    await record({ phase: "startup", error: String(error) }).catch(console.error);
    await cleanup();
    throw error;
  }
}

main().catch((error) => {
  console.error(String(error));
  process.exitCode = 1;
});
