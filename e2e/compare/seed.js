import fs from "node:fs/promises";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

// Synthetic media of a comparison scenario. `control` is exactly what `app:control start`
// indexes for the Tauri application (e2e/control.js): four PNGs, one GIF and two MP4s with the
// same file names, so both applications see the same library. `empty` is a library with no
// scan root; the Tauri side reaches it by removing the seeded root through the UI.
export const SEEDS = Object.freeze(["empty", "control"]);
export const CONTROL_PNG_PREFIX = "control";
export const CONTROL_PNG_COUNT = 4;
export const CONTROL_PLAYABLE_PREFIX = "control-playable";

export function validateSeed(seed) {
  if (!SEEDS.includes(seed))
    throw new Error(`Unknown seed ${JSON.stringify(seed)}; expected one of ${SEEDS.join(", ")}`);
  return seed;
}

// Callers own `root`: a fresh, empty temporary directory, removed by them even on failure.
export async function seedMedia(root, seed) {
  validateSeed(seed);
  if ((await fs.readdir(root)).length)
    throw new Error(`Refusing to seed a directory that is not empty: ${root}`);
  if (seed === "empty") return [];
  await copyPngFixtures(root, CONTROL_PNG_PREFIX, CONTROL_PNG_COUNT);
  await createPlayableFixtures(root, CONTROL_PLAYABLE_PREFIX);
  return (await fs.readdir(root)).sort();
}
