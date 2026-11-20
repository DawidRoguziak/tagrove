import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Builds the GPUI adapter for the isolated E2E profile (ADR 0036), the counterpart of
// e2e/build-e2e.js for the Tauri adapter. The identifier is baked in at build time
// (src-gpui/crates/media_tagger_gpui/build.rs) and the build has its own target directory,
// so the binary of `bun run gpui:dev` is never replaced and never opens the E2E profile.

const currentFile = fileURLToPath(import.meta.url);

export const projectRoot = path.resolve(path.dirname(currentFile), "..");
export const gpuiManifestPath = path.resolve(projectRoot, "src-gpui", "Cargo.toml");
export const gpuiE2eTargetDir = path.resolve(projectRoot, "src-gpui", "target-e2e");
export const gpuiE2eBinaryPath = path.resolve(gpuiE2eTargetDir, "debug", "media_tagger_gpui");
export const gpuiE2eIdentifier = "com.example.mediatagger.gpui.e2e";
export const gpuiE2eWindowTitle = "Tagrove GPUI E2E";

export function validateGpuiE2eBuildConfiguration() {
  const profilePath = path.resolve(
    projectRoot,
    "src-gpui",
    "crates",
    "media_tagger_gpui",
    "src",
    "profile.rs"
  );
  const profileSource = fs.readFileSync(profilePath, "utf8");

  if (
    !gpuiE2eIdentifier.endsWith(".gpui.e2e") ||
    !profileSource.includes(`"${gpuiE2eIdentifier}"`)
  ) {
    throw new Error(
      `Refusing GPUI E2E build: profile.rs does not allow the isolated identifier ${gpuiE2eIdentifier}`
    );
  }

  if (!profileSource.includes(`"${gpuiE2eWindowTitle}"`)) {
    throw new Error(`Refusing GPUI E2E build without isolated window title: ${gpuiE2eWindowTitle}`);
  }

  const devTargetDir = path.resolve(projectRoot, "src-gpui", "target");
  if (gpuiE2eTargetDir === devTargetDir || !gpuiE2eTargetDir.endsWith(`${path.sep}target-e2e`)) {
    throw new Error(`Refusing GPUI E2E build with unsafe target directory: ${gpuiE2eTargetDir}`);
  }
}

export function buildGpuiE2eApp() {
  validateGpuiE2eBuildConfiguration();
  fs.rmSync(gpuiE2eBinaryPath, { force: true });

  const result = spawnSync(
    "cargo",
    ["build", "--manifest-path", gpuiManifestPath, "--locked", "-p", "media_tagger_gpui"],
    {
      cwd: projectRoot,
      stdio: "inherit",
      env: {
        ...process.env,
        CARGO_TARGET_DIR: gpuiE2eTargetDir,
        MEDIATAGGER_GPUI_IDENTIFIER: gpuiE2eIdentifier
      }
    }
  );

  if (result.status !== 0) {
    throw new Error("GPUI E2E build failed");
  }

  if (!fs.existsSync(gpuiE2eBinaryPath)) {
    throw new Error(`GPUI E2E build did not create the isolated binary: ${gpuiE2eBinaryPath}`);
  }

  // The binary answers `--identifier` before it touches a display or the disk.
  const baked = spawnSync(gpuiE2eBinaryPath, ["--identifier"], { encoding: "utf8" });
  const bakedIdentifier = baked.status === 0 ? baked.stdout.trim() : "";
  if (bakedIdentifier !== gpuiE2eIdentifier) {
    fs.rmSync(gpuiE2eBinaryPath, { force: true });
    throw new Error(
      `GPUI E2E build produced a binary for ${JSON.stringify(bakedIdentifier)} instead of ${gpuiE2eIdentifier}`
    );
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === currentFile) {
  buildGpuiE2eApp();
}
