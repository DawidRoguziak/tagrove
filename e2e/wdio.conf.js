import os from "node:os";
import path from "node:path";
import fs from "node:fs";
import { spawn } from "node:child_process";
import {
  buildE2eApp,
  e2eAppIdentifier,
  e2eBinaryPath,
  e2eWindowTitle,
  productionAppIdentifier,
  validateE2eBuildConfiguration
} from "./build-e2e.js";



let tauriDriver;
let shuttingDown = false;
let e2eStartedAt;

function formatDuration(durationMs) {
  return `${(durationMs / 1000).toFixed(1)}s`;
}

function resolveTauriDriverPath() {
  if (process.env.TAURI_DRIVER_PATH) {
    return process.env.TAURI_DRIVER_PATH;
  }

  return path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver");
}

function resolveNativeDriverPath() {
  return process.env.WEBKIT_WEBDRIVER_PATH ?? "/usr/bin/WebKitWebDriver";
}

function resolveAppDataRoot() {
  return process.env.XDG_DATA_HOME ?? path.resolve(os.homedir(), ".local", "share");
}

function clearE2eAppDataDir() {
  const appDataRoot = path.resolve(resolveAppDataRoot());
  const appDataDir = path.resolve(appDataRoot, e2eAppIdentifier);
  const productionAppDataDir = path.resolve(appDataRoot, productionAppIdentifier);

  if (
    appDataDir === productionAppDataDir ||
    path.dirname(appDataDir) !== appDataRoot ||
    path.basename(appDataDir) !== e2eAppIdentifier ||
    !e2eAppIdentifier.endsWith(".e2e")
  ) {
    throw new Error(`Refusing unsafe E2E app data cleanup: ${appDataDir}`);
  }

  try {
    fs.rmSync(appDataDir, {
      recursive: true,
      force: true,
      maxRetries: 5,
      retryDelay: 200
    });
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "EPERM") {
      console.warn(`Skipping cleanup for locked E2E app data directory: ${appDataDir}`);
      return;
    }

    throw error;
  }
}

function closeTauriDriver() {
  shuttingDown = true;
  if (tauriDriver) {
    tauriDriver.kill();
    tauriDriver = undefined;
  }
}

async function getNativeWindowTitle() {
  const response = await browser.executeAsync((done) => {
    const tauri =
      window.__TAURI__?.core && typeof window.__TAURI__.core.invoke === "function"
        ? window.__TAURI__.core
        : window.__TAURI_INTERNALS__ && typeof window.__TAURI_INTERNALS__.invoke === "function"
          ? window.__TAURI_INTERNALS__
          : null;

    if (!tauri) {
      done({ ok: false, error: "Tauri invoke bridge is not available in test runtime." });
      return;
    }

    Promise.resolve()
      .then(() => tauri.invoke("plugin:window|title", { label: "main" }))
      .then((value) => done({ ok: true, value }))
      .catch((error) => done({ ok: false, error: String(error) }));
  });

  if (!response?.ok || typeof response.value !== "string") {
    throw new Error(response?.error ?? "Failed to read the native Tauri window title.");
  }

  return response.value;
}

// Exit status of a worker stopped by the live-title guard (EX_CONFIG).
const guardAbortExitCode = 78;
const signalExitCodes = { SIGHUP: 129, SIGINT: 130, SIGTERM: 143 };

function onShutdown(fn) {
  // A normal exit already carries its code; only run the cleanup.
  process.on("exit", () => {
    fn();
  });

  // An interrupted run must not report success to `&&` chains such as `test:all`.
  // Prepend: the create-wdio SIGINT handler loaded by the WDIO CLI exits with status 1 synchronously.
  for (const [signal, code] of Object.entries(signalExitCodes)) {
    process.prependListener(signal, () => {
      try {
        fn();
      } finally {
        process.exit(code);
      }
    });
  }

  // A worker whose launcher has gone away (killed or exited on a signal) is orphaned together with
  // its tauri-driver and the app; stop them instead of leaving the display session occupied.
  if (typeof process.send === "function") {
    process.once("disconnect", () => {
      try {
        fn();
      } finally {
        process.exit(1);
      }
    });
  }
}

// WebdriverIO logs a rejected `before` hook and still runs the specs, so the live-title guard
// has to end the worker itself. A worker that exits non-zero fails the whole run.
export function abortRun(message, exit = process.exit) {
  console.error(message);
  try {
    closeTauriDriver();
  } finally {
    exit(guardAbortExitCode);
  }
}

// The launcher keeps scheduling workers for the remaining spec files after a failed one. A worker
// that ended on the title guard must end the whole run, so `onWorkerEnd` recognizes its exit code.
export function stopRunAfterGuardAbort(exitCode, exit = process.exit) {
  if (exitCode === guardAbortExitCode) {
    console.error("[E2E] ABORT The live window title guard failed; skipping the remaining specs");
    exit(1);
  }
}

export async function assertLiveWindowTitle(readTitle = getNativeWindowTitle, exit = process.exit) {
  let windowTitle;
  try {
    windowTitle = await readTitle();
  } catch (error) {
    abortRun(`Refusing destructive E2E suite, the live window title is unreadable: ${error?.message ?? error}`, exit);
    return false;
  }
  if (windowTitle !== e2eWindowTitle) {
    abortRun(`Refusing destructive E2E suite for unexpected application window: ${windowTitle}`, exit);
    return false;
  }
  return true;
}

export const config = {
  host: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.e2e.js"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: e2eBinaryPath
      }
    }
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: {
    ui: "bdd",
    timeout: 90000
  },
  onPrepare: () => {
    e2eStartedAt = Date.now();
    console.log("[E2E] START Preparing MediaTagger desktop suite");
    try {
      validateE2eBuildConfiguration();
      fs.rmSync(e2eBinaryPath, { force: true });
      clearE2eAppDataDir();
      buildE2eApp();
    } catch (error) {
      // WDIO logs a rejected onPrepare hook but still starts workers.
      console.error(error);
      process.exit(1);
    }
    console.log("[E2E] READY Desktop application build completed");
  },
  beforeSession: () => {
    console.log("[E2E] DRIVER Starting Tauri driver");
    const nativeDriverPath = resolveNativeDriverPath();
    const args = nativeDriverPath ? ["--native-driver", nativeDriverPath] : [];

    tauriDriver = spawn(resolveTauriDriverPath(), args, {
      stdio: [null, process.stdout, process.stderr],
      env: {
        ...process.env,
        LANG: process.env.LANG ?? "C.UTF-8",
        LC_ALL: process.env.LC_ALL ?? "C.UTF-8",
        LANGUAGE: "en"
      }
    });

    tauriDriver.on("error", (error) => {
      console.error("tauri-driver failed to start:", error);
      process.exit(1);
    });

    tauriDriver.on("exit", (code) => {
      if (!shuttingDown) {
        console.error("tauri-driver exited unexpectedly with code:", code);
        process.exit(1);
      }
    });
  },
  before: async () => {
    if (!(await assertLiveWindowTitle())) {
      return;
    }
    // Each spec starts from the same preferences. Reloads within a spec retain them.
    await browser.execute(() => {
      localStorage.setItem("media-tagger.language", "en");
      localStorage.setItem("media-tagger.theme", "dark");
    });
    await browser.refresh();
  },
  onWorkerEnd: (_cid, exitCode) => {
    stopRunAfterGuardAbort(exitCode);
  },
  afterSession: () => {
    closeTauriDriver();
    console.log("[E2E] DRIVER Stopped Tauri driver");
  },
  beforeTest: (test) => {
    console.log(`[E2E] RUN ${test.parent} > ${test.title}`);
  },
  afterTest: (test, _context, { error, duration, passed }) => {
    const status = passed ? "PASS" : "FAIL";
    console.log(`[E2E] ${status} ${test.parent} > ${test.title} (${formatDuration(duration)})`);

    if (error) {
      console.error(`[E2E] ERROR ${error.message ?? String(error)}`);
    }
  },
  onComplete: (exitCode) => {
    const duration = e2eStartedAt ? ` after ${formatDuration(Date.now() - e2eStartedAt)}` : "";
    const status = exitCode === 0 ? "PASS" : "FAIL";
    console.log(`[E2E] COMPLETE ${status} (exit code ${exitCode})${duration}`);
  }
};

onShutdown(() => {
  closeTauriDriver();
});


