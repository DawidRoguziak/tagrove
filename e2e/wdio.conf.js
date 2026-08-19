import os from "node:os";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { spawn, spawnSync } from "node:child_process";
import {
  buildE2eApp,
  e2eAppIdentifier,
  e2eBinaryPath,
  e2eWindowTitle,
  productionAppIdentifier,
  validateE2eBuildConfiguration
} from "./build-e2e.js";

const __dirname = fileURLToPath(new URL(".", import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

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

  const exeSuffix = process.platform === "win32" ? ".exe" : "";
  return path.resolve(os.homedir(), ".cargo", "bin", `tauri-driver${exeSuffix}`);
}

function resolveNativeDriverPath() {
  if (process.platform !== "win32") {
    return process.env.WEBKIT_WEBDRIVER_PATH ?? "/usr/bin/WebKitWebDriver";
  }

  if (process.env.MSEDGEDRIVER_PATH) {
    return process.env.MSEDGEDRIVER_PATH;
  }

  const localPath = path.resolve(projectRoot, "msedgedriver.exe");
  if (fs.existsSync(localPath)) {
    return localPath;
  }

  return null;
}

function runOrFail(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: "inherit",
    shell: true
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(" ")}`);
  }
}

function resolveAppDataRoot() {
  if (process.platform === "win32") {
    return process.env.APPDATA ?? path.resolve(os.homedir(), "AppData", "Roaming");
  }

  if (process.platform === "darwin") {
    return path.resolve(os.homedir(), "Library", "Application Support");
  }

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

function onShutdown(fn) {
  const cleanup = () => {
    try {
      fn();
    } finally {
      process.exit();
    }
  };

  process.on("exit", cleanup);
  process.on("SIGINT", cleanup);
  process.on("SIGTERM", cleanup);
  process.on("SIGHUP", cleanup);
  process.on("SIGBREAK", cleanup);
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
    validateE2eBuildConfiguration();
    fs.rmSync(e2eBinaryPath, { force: true });
    clearE2eAppDataDir();
    runOrFail("bun", ["run", "build"], projectRoot);
    buildE2eApp();
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
    const windowTitle = await getNativeWindowTitle();
    if (windowTitle !== e2eWindowTitle) {
      throw new Error(
        `Refusing destructive E2E suite for unexpected application window: ${windowTitle}`
      );
    }
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


