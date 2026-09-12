import { execFile } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);
const iconsRoot = fileURLToPath(new URL("../src-tauri/icons", import.meta.url));

// Callers own the temporary destination and its cleanup, including on failure.
export async function copyPngFixtures(root, prefix, count) {
  const sources = (await fs.readdir(iconsRoot)).filter((name) =>
    name.toLowerCase().endsWith(".png")
  );
  if (!sources.length) throw new Error("Cannot seed media: no PNG source files found.");
  for (let index = 0; index < count; index++) {
    await fs.copyFile(
      path.join(iconsRoot, sources[index % sources.length]),
      path.join(root, `${prefix}-${index + 1}.png`)
    );
  }
}

export async function createPlayableFixtures(root, prefix) {
  const gifPath = path.join(root, `${prefix}.gif`);
  const videoPaths = [1, 2].map((index) => path.join(root, `${prefix}-${index}.mp4`));
  const ffmpeg = process.env.FFMPEG_PATH ?? "ffmpeg";
  await execFileAsync(ffmpeg, [
    "-hide_banner",
    "-loglevel",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc=size=96x64:rate=8",
    "-t",
    "1",
    "-y",
    gifPath
  ]);
  for (const [index, videoPath] of videoPaths.entries()) {
    await execFileAsync(ffmpeg, [
      "-hide_banner",
      "-loglevel",
      "error",
      "-f",
      "lavfi",
      "-i",
      "testsrc2=size=160x90:rate=24",
      "-t",
      "8",
      "-c:v",
      "mpeg4",
      "-q:v",
      "5",
      "-an",
      ...(index === 1 ? ["-movflags", "+faststart"] : []),
      "-y",
      videoPath
    ]);
  }
  return { gifPath, videoPaths };
}

export async function createLightboxFixtures(root) {
  for (const [name, size] of [["portrait", "800x4800"], ["panorama", "4800x400"], ["large", "4800x3200"]]) {
    await execFileAsync(process.env.FFMPEG_PATH ?? "ffmpeg", [
      "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", `testsrc2=size=${size}`,
      "-frames:v", "1", "-y", path.join(root, `${name}-${"long_filename_".repeat(10)}.png`)
    ]);
  }
}
