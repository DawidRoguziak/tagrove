import fs from "node:fs/promises";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

// Synthetic media of a comparison scenario. `control` is exactly what `app:control start`
// indexes for the Tauri application (e2e/control.js): four PNGs, one GIF and two MP4s with the
// same file names, so both applications see the same library. `empty` is a library with no
// scan root; the Tauri side reaches it by removing the seeded root through the UI.
export const SEEDS = Object.freeze(["empty", "control"]);
export const CONTROL_PNG_PREFIX = "control";
export const CONTROL_PNG_COUNT = 4;
export const CONTROL_PLAYABLE_PREFIX = "control-playable";

// The gallery order of the control seed, first tile first. The gallery sorts by modification
// time in whole seconds (newest first) and then by asset ID, and the seed is written within about
// a second while a scan gives IDs in the order its workers finish, so both seeding paths pin the
// order with `pinControlSeedOrder` before the application scans the root. The GIF comes first so
// the first tile is not one of the four PNGs (search-applied keeps "4 items" whether or not GIFs
// count as images), and the first three tiles have names that stay apart on the order board.
export const CONTROL_SEED_ORDER = Object.freeze([
  `${CONTROL_PLAYABLE_PREFIX}.gif`,
  ...Array.from(
    { length: CONTROL_PNG_COUNT },
    (_, index) => `${CONTROL_PNG_PREFIX}-${index + 1}.png`
  ),
  `${CONTROL_PLAYABLE_PREFIX}-1.mp4`,
  `${CONTROL_PLAYABLE_PREFIX}-2.mp4`
]);
// 2020-01-01T12:30:00Z: fixed and in the past, so the run's clock changes nothing. The files lie
// one minute apart, newest first, between 12:24 and 12:30 UTC, which is one calendar day in every
// time zone from UTC-12 to UTC+14.
export const CONTROL_SEED_BASE_SECONDS = 1_577_881_800;
export const CONTROL_SEED_STEP_SECONDS = 60;

export function controlSeedModifiedSeconds(name) {
  const rank = CONTROL_SEED_ORDER.indexOf(name);
  if (rank < 0) throw new Error(`Not a control seed file: ${JSON.stringify(name)}`);
  return CONTROL_SEED_BASE_SECONDS - rank * CONTROL_SEED_STEP_SECONDS;
}

// Sets the access and modification time of every control seed file in `root` from its name.
// `root` must hold exactly the control seed, so the rule never touches anything else.
export async function pinControlSeedOrder(root) {
  const names = (await fs.readdir(root)).sort();
  const expected = [...CONTROL_SEED_ORDER].sort();
  if (names.length !== expected.length || names.some((name, index) => name !== expected[index]))
    throw new Error(
      `Refusing to pin a directory that does not hold exactly the control seed: ${root}`
    );
  for (const name of CONTROL_SEED_ORDER) {
    const seconds = controlSeedModifiedSeconds(name);
    await fs.utimes(path.join(root, name), seconds, seconds);
  }
}

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
  await pinControlSeedOrder(root);
  return (await fs.readdir(root)).sort();
}
