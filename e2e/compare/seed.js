import fs from "node:fs/promises";
import path from "node:path";
import { copyPngFixtures, createPlayableFixtures } from "../fixtures.js";

// Synthetic media of a comparison scenario. `control` is exactly what `app:control start`
// indexes for the Tauri application (e2e/control.js): four PNGs, one GIF and two MP4s with the
// same file names, so both applications see the same library. `empty` is a library with no
// scan root; the Tauri side reaches it by removing the seeded root through the UI. `duplicates`
// is `control` and one more file with the name of the first PNG in a subfolder of the root, so
// the library holds one duplicate group of two files (`addControlDuplicate`); the Tauri side
// gets that file added to the root `app:control start` indexed and rescans through the UI.
export const SEEDS = Object.freeze(["empty", "control", "duplicates"]);
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

// The duplicate of the `duplicates` seed: `<root>/copy/control-1.png`. It is older than every
// control seed file, so the seven control tiles keep their places and it is the last tile.
export const CONTROL_DUPLICATE_DIRECTORY = "copy";
export const CONTROL_DUPLICATE_NAME = `${CONTROL_PNG_PREFIX}-1.png`;
export const CONTROL_DUPLICATE_SECONDS =
  CONTROL_SEED_BASE_SECONDS - CONTROL_SEED_ORDER.length * CONTROL_SEED_STEP_SECONDS;

export function controlSeedModifiedSeconds(name) {
  const rank = CONTROL_SEED_ORDER.indexOf(name);
  if (rank < 0) throw new Error(`Not a control seed file: ${JSON.stringify(name)}`);
  return CONTROL_SEED_BASE_SECONDS - rank * CONTROL_SEED_STEP_SECONDS;
}

async function holdsExactlyTheControlSeed(root) {
  const names = (await fs.readdir(root)).sort();
  const expected = [...CONTROL_SEED_ORDER].sort();
  return names.length === expected.length && names.every((name, index) => name === expected[index]);
}

// Sets the access and modification time of every control seed file in `root` from its name.
// `root` must hold exactly the control seed, so the rule never touches anything else.
export async function pinControlSeedOrder(root) {
  if (!(await holdsExactlyTheControlSeed(root)))
    throw new Error(
      `Refusing to pin a directory that does not hold exactly the control seed: ${root}`
    );
  for (const name of CONTROL_SEED_ORDER) {
    const seconds = controlSeedModifiedSeconds(name);
    await fs.utimes(path.join(root, name), seconds, seconds);
  }
}

// Adds the duplicate of the `duplicates` seed to `root`: a copy of the first PNG under the same
// name in the subfolder `copy`, with a fixed modification time. `root` must hold exactly the
// control seed, so nothing is ever written into another directory. Returns the path of the new
// file relative to `root`.
export async function addControlDuplicate(root) {
  if (!(await holdsExactlyTheControlSeed(root)))
    throw new Error(
      `Refusing to add a duplicate to a directory that does not hold exactly the control seed: ${root}`
    );
  const directory = path.join(root, CONTROL_DUPLICATE_DIRECTORY);
  await fs.mkdir(directory);
  const target = path.join(directory, CONTROL_DUPLICATE_NAME);
  await fs.copyFile(path.join(root, CONTROL_DUPLICATE_NAME), target);
  await fs.utimes(target, CONTROL_DUPLICATE_SECONDS, CONTROL_DUPLICATE_SECONDS);
  return path.join(CONTROL_DUPLICATE_DIRECTORY, CONTROL_DUPLICATE_NAME);
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
  if (seed === "duplicates") await addControlDuplicate(root);
  return (await fs.readdir(root)).sort();
}
