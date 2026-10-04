import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import solid from "@solidjs/vite-plugin";

// These suites do not mount a renderer. Keep one copy during migration.
const sharedTests = [
  "src/components/**/services/__tests__/*.test.ts",
  "src/hooks/services/__tests__/*.test.ts",
  "src/utils/__tests__/*.test.ts",
  "src/platform/tauri/__tests__/api.test.ts",
  "src/ui/dom/__tests__/*.test.ts",
  "src/components/lightbox/__tests__/nativeVideoLayout.test.ts"
];
const solidTests = ["src/{app,platform,library,features,ui}/**/*.{test,spec}.{ts,tsx}"];
const common = {
  environment: "jsdom",
  css: true,
  globals: false,
  pool: "forks" as const,
  execArgv: ["--max-old-space-size=8192"]
};

export default defineConfig({
  test: {
    maxWorkers: 2,
    projects: [
      {
        test: {
          ...common,
          name: "shared",
          include: sharedTests,
          setupFiles: "./src/test/setup-dom.ts"
        }
      },
      {
        plugins: [react()],
        test: {
          ...common,
          name: "react",
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
          exclude: [...sharedTests, ...solidTests],
          setupFiles: "./src/test/setup.ts"
        }
      },
      {
        plugins: [solid()],
        resolve: { conditions: ["browser", "development"] },
        test: {
          ...common,
          name: "solid",
          include: solidTests,
          exclude: sharedTests,
          setupFiles: "./src/test/setup-solid.ts",
          server: { deps: { inline: [/solid-js/, /@solidjs\//] } }
        }
      }
    ]
  }
});
