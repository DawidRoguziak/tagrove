import { defineConfig } from "vite";
import solid from "@solidjs/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL(".", import.meta.url));
export default defineConfig({ root, base: "./", cacheDir: `${root}../../artifacts/gallery-scale/.vite`, publicDir: false,
  plugins: [{ name: "saved-gallery-baseline", enforce: "pre", load(id) {
    if (process.env.GALLERY_BASELINE_DIR && ["/src/features/gallery/createVirtualGrid.ts", "/src/features/gallery/GalleryGrid.tsx", "/src/library/createLibrary.ts"].some(path => id.endsWith(path)))
      return readFileSync(`${process.env.GALLERY_BASELINE_DIR}/${basename(id)}`, "utf8");
  } }, solid(), tailwindcss()], build: { rollupOptions: { input: { index: `${root}index.html`, performance: `${root}performance.html` } }, outDir: `${root}../../artifacts/gallery-scale/dist`, emptyOutDir: true } });
