import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const currentFile = fileURLToPath(import.meta.url);
const e2eDir = path.dirname(currentFile);

export const projectRoot = path.resolve(e2eDir, "..");
export const e2eTargetDir = path.resolve(projectRoot, "src-tauri", "target-e2e");
const executableName = process.platform === "win32" ? "media_tagger.exe" : "media_tagger";
export const e2eBinaryPath = path.resolve(e2eTargetDir, "debug", executableName);
export const e2eAppIdentifier = "com.example.mediatagger.e2e";
export const e2eWindowTitle = "Image Viewer 3000 E2E";
export const productionAppIdentifier = "com.example.mediatagger";

export function validateE2eBuildConfiguration() {
  const configPath = path.resolve(projectRoot, "src-tauri", "tauri.conf.e2e.json");
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));

  if (config.identifier !== e2eAppIdentifier || !config.identifier.endsWith(".e2e")) {
    throw new Error(
      `Refusing E2E build: expected isolated identifier ${e2eAppIdentifier}, received ${config.identifier}`
    );
  }

  const configuredWindowTitles = config.app?.windows?.map((window) => window.title) ?? [];
  if (!configuredWindowTitles.includes(e2eWindowTitle)) {
    throw new Error(`Refusing E2E build without isolated window title: ${e2eWindowTitle}`);
  }

  const productionTargetDir = path.resolve(projectRoot, "src-tauri", "target");
  if (e2eTargetDir === productionTargetDir || !e2eTargetDir.endsWith(`${path.sep}target-e2e`)) {
    throw new Error(`Refusing E2E build with unsafe target directory: ${e2eTargetDir}`);
  }
}

export function buildE2eApp() {
  validateE2eBuildConfiguration();
  fs.rmSync(e2eBinaryPath, { force: true });

  const result = spawnSync(
    "tauri",
    ["build", "--debug", "--no-bundle", "--config", "src-tauri/tauri.conf.e2e.json"],
    {
      cwd: projectRoot,
      stdio: "inherit",
      shell: true,
      env: {
        ...process.env,
        CARGO_TARGET_DIR: e2eTargetDir
      }
    }
  );

  if (result.status !== 0) {
    throw new Error("E2E Tauri build failed");
  }

  if (!fs.existsSync(e2eBinaryPath)) {
    throw new Error(`E2E build did not create the isolated binary: ${e2eBinaryPath}`);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === currentFile) {
  buildE2eApp();
}
