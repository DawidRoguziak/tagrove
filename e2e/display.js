import { execFile } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { promisify } from "node:util";

const execFileAsync = promisify(execFile);

// Native screenshots (`import -window root`) read the whole X screen, so on a developer
// desktop they would capture unrelated windows into artifacts/. Every spec capture goes
// through captureDisplay, which refuses unless the display is the run's own private Xvfb:
// - DISPLAY is a local `:N`, equal to MEDIATAGGER_NATIVE_DISPLAY, and not :0 or :1;
// - XAUTHORITY is an absolute path;
// - a live Xvfb owned by this user serves that display with exactly that authority file,
//   either as `-auth` (app:control, older xvfb-run) or as XAUTHORITY in its environment
//   (newer xvfb-run).
// The checks run before any process touches the display and throw; they never skip.
export function assertPrivateDisplay(env = process.env, { procRoot = "/proc", uid = process.getuid?.() } = {}) {
  const display = env.DISPLAY;
  const match = /^:(\d+)$/.exec(display ?? "");
  if (!match) throw new Error(`Refusing native capture: DISPLAY ${JSON.stringify(display)} is not a local :N display.`);
  if (display !== env.MEDIATAGGER_NATIVE_DISPLAY)
    throw new Error("Refusing native capture: DISPLAY is not MEDIATAGGER_NATIVE_DISPLAY (the run's private display).");
  if (Number(match[1]) <= 1) throw new Error(`Refusing native capture: ${display} may be a real desktop.`);
  const authority = env.XAUTHORITY;
  if (!authority || !path.isAbsolute(authority))
    throw new Error("Refusing native capture: XAUTHORITY must name the private display's authority file.");
  if (!ownedXvfbServes(display, authority, procRoot, uid))
    throw new Error(`Refusing native capture: no Xvfb owned by this user serves ${display} with authority ${authority}.`);
}

function ownedXvfbServes(display, authority, procRoot, uid) {
  let entries;
  try {
    entries = fs.readdirSync(procRoot);
  } catch {
    return false;
  }
  for (const pid of entries) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const dir = path.join(procRoot, pid);
      if (uid !== undefined && fs.statSync(dir).uid !== uid) continue;
      const argv = fs.readFileSync(path.join(dir, "cmdline"), "utf8").split("\0");
      if (path.basename(argv[0] ?? "") !== "Xvfb" || argv[1] !== display) continue;
      const auth = argv.indexOf("-auth");
      if (auth > 0 && argv[auth + 1] === authority) return true;
      // Newer xvfb-run starts Xvfb with XAUTHORITY in its environment instead of `-auth`.
      const environ = fs.readFileSync(path.join(dir, "environ"), "utf8").split("\0");
      if (environ.includes(`XAUTHORITY=${authority}`)) return true;
    } catch {
      // The process exited while scanning; keep looking.
    }
  }
  return false;
}

// Capture the private display root (optionally cropped) into `file`.
export async function captureDisplay(file, { crop } = {}) {
  assertPrivateDisplay();
  const args = ["-display", process.env.DISPLAY, "-window", "root"];
  if (crop) args.push("-crop", crop);
  await execFileAsync("import", [...args, file]);
}
