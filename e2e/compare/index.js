// `node --test e2e/compare` resolves the directory to this file, which loads every test file
// next to it, so a new *.test.js needs no registration.
import fs from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";

const directory = fileURLToPath(new URL(".", import.meta.url));
for (const name of (await fs.readdir(directory)).filter((file) => file.endsWith(".test.js")).sort())
  await import(pathToFileURL(`${directory}${name}`).href);
