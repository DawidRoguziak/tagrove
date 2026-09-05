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

export function validateRequest(value) {
  const arities = {
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
    diagnose: [0, 0]
  };
  const limits = Object.hasOwn(arities, value?.command) ? arities[value.command] : null;
  if (
    !limits ||
    !Array.isArray(value.args) ||
    value.args.length < limits[0] ||
    value.args.length > limits[1] ||
    value.args.some((arg) => typeof arg !== "string" || arg.length > 16000)
  ) {
    throw new Error("Invalid control command or arguments. Run app:control help.");
  }
  return value;
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
