import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import solid from "@solidjs/vite-plugin";
import { selectFrontend } from "./scripts/frontend.mjs";

export default defineConfig(() => {
  const frontend = selectFrontend();
  return {
    plugins: [
      frontend === "solid" ? solid() : react(),
      tailwindcss(),
      {
        name: "mediatagger-frontend-entry",
        transformIndexHtml: {
          order: "pre",
          handler: (html) =>
            frontend === "solid"
              ? html.replace('src="/src/main.tsx"', 'src="/src/app/main.tsx"')
              : html
        }
      }
    ],
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true
    }
  };
});
