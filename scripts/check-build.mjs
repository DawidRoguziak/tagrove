import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { build } from "vite";

const result = await build();
const outputs = Array.isArray(result) ? result : [result];
const modules = outputs.flatMap((output) =>
  output.output.flatMap((chunk) => (chunk.type === "chunk" ? Object.keys(chunk.modules) : []))
);
assert.ok(modules.some((id) => /\/node_modules\/(solid-js|@solidjs)\//.test(id)));
assert.deepEqual(
  modules.filter((id) => /\/node_modules\/(react(?:-[^/]+)?|@tanstack\/react-virtual)\//.test(id)),
  [],
  "React must not return to the production bundle"
);
assert.ok(modules.some((id) => id.endsWith("/src/app/main.tsx")));
assert.deepEqual(
  modules.filter(
    (id) => /\/src\/(components|hooks)\//.test(id) || /\/src\/(main|App)\.tsx$/.test(id)
  ),
  [],
  "Legacy entry and feature trees are absent"
);
const manifest = JSON.parse(await fs.readFile("package.json", "utf8"));
for (const dependencies of [manifest.dependencies, manifest.devDependencies]) {
  for (const name of Object.keys(dependencies)) {
    assert.doesNotMatch(
      name,
      /^(react(?:-dom|-i18next|-refresh)?|babel-plugin-react-compiler|oxc-transform-react|@types\/react(?:-dom)?|@vitejs\/plugin-react|@testing-library\/react|@tanstack\/react-virtual)$/
    );
  }
}
const html = await fs.readFile("dist/index.html", "utf8");
assert.match(html, /<script[^>]+type="module"[^>]+src="\/assets\//);
assert.doesNotMatch(html, /src="\/src\//);
await fs.mkdir("artifacts/build", { recursive: true });
await fs.writeFile(
  "artifacts/build/modules.json",
  JSON.stringify(
    modules.map((id) => path.relative(process.cwd(), id)),
    null,
    2
  )
);
console.log(`${modules.length} modules; Solid entry and production dependency checks passed`);
