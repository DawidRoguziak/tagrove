import { invoke } from "@tauri-apps/api/core";

export async function syncNativeWindowTheme(theme: "light" | "dark"): Promise<void> {
  await invoke("sync_window_theme", { theme });
}

export { isTauri } from "@tauri-apps/api/core";
export { getCurrentWindow } from "@tauri-apps/api/window";
