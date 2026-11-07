import { defineConfig } from "vite";
import solid from "@solidjs/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
const root = fileURLToPath(new URL(".", import.meta.url));
export default defineConfig({ root, base: "./", cacheDir: `${root}../../artifacts/gallery-scale/.vite`, publicDir: false,
  plugins: [solid(), tailwindcss()], build: { rollupOptions: { input: { index: `${root}index.html`, performance: `${root}performance.html`, heightProbe: `${root}height-probe.html` } }, outDir: `${root}../../artifacts/gallery-scale/dist`, emptyOutDir: true } });
