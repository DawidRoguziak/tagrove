import { defineConfig } from "vitest/config";
import solid from "@solidjs/vite-plugin";

export default defineConfig({
  plugins: [solid()],
  resolve: { conditions: ["browser", "development"] },
  test: {
    environment: "jsdom",
    css: true,
    globals: false,
    pool: "forks",
    maxWorkers: 2,
    execArgv: ["--max-old-space-size=8192"],
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    setupFiles: "./src/test/setup.ts",
    server: { deps: { inline: [/solid-js/, /@solidjs\//] } }
  }
});
