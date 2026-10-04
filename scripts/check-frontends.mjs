import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import { build } from "vite";
import { selectFrontend } from "./frontend.mjs";

const previous = process.env.MEDIATAGGER_FRONTEND;
delete process.env.MEDIATAGGER_FRONTEND;
assert.equal(selectFrontend(), "react");
for (const value of ["", "Solid", "vue"]) {
  assert.throws(() => selectFrontend(value), /Unsupported MEDIATAGGER_FRONTEND/);
}

try {
  for (const frontend of ["solid", "react"]) {
    process.env.MEDIATAGGER_FRONTEND = frontend;
    const result = await build();
    const outputs = Array.isArray(result) ? result : [result];
    const modules = outputs.flatMap((output) =>
      output.output.flatMap((chunk) => (chunk.type === "chunk" ? Object.keys(chunk.modules) : []))
    );
    const react = modules.filter((id) =>
      /\/node_modules\/(react|react-dom|react-i18next)\//.test(id)
    );
    const solid = modules.filter((id) => /\/node_modules\/(solid-js|@solidjs)\//.test(id));
    assert.ok((frontend === "react" ? react : solid).length > 0, "selected renderer is bundled");
    assert.deepEqual(frontend === "react" ? solid : react, [], "other renderer is absent");
    assert.ok(
      modules.some((id) =>
        id.endsWith(frontend === "react" ? "/src/main.tsx" : "/src/app/main.tsx")
      )
    );
    assert.ok(
      !modules.some((id) =>
        id.endsWith(frontend === "solid" ? "/src/main.tsx" : "/src/app/main.tsx")
      )
    );
    const html = await fs.readFile("dist/index.html", "utf8");
    assert.match(html, /<script[^>]+type="module"[^>]+src="\/assets\//);
    assert.doesNotMatch(html, /src="\/src\//);
    await fs.mkdir("artifacts/solid2/stage1", { recursive: true });
    await fs.writeFile(
      `artifacts/solid2/stage1/${frontend}-modules.json`,
      JSON.stringify(
        modules.map((id) => path.relative(process.cwd(), id)),
        null,
        2
      )
    );
    console.log(`${frontend}: ${modules.length} modules; renderer and HTML isolation passed`);
  }
} finally {
  if (previous === undefined) delete process.env.MEDIATAGGER_FRONTEND;
  else process.env.MEDIATAGGER_FRONTEND = previous;
}
