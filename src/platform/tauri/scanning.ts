import { invoke } from "@tauri-apps/api/core";
import type { ScanRoot, ScanSummary, RemoveRootSummary } from "./contracts/scanning";

export async function scanFolder(path: string): Promise<ScanSummary> {
  return await invoke<ScanSummary>("scan_folder", { path });
}

export async function listScanRoots(): Promise<ScanRoot[]> {
  return await invoke<ScanRoot[]>("list_scan_roots");
}

export async function setScanRootAutoScan(path: string, enabled: boolean): Promise<void> {
  return await invoke<void>("set_scan_root_auto_scan", { path, enabled });
}

// Joins the process-owned startup scan, including its retained success or failure.
// Null means the launch snapshot had no enabled roots.
export async function scanStartupRoots(): Promise<ScanSummary | null> {
  return await invoke<ScanSummary | null>("scan_startup_roots");
}

export async function addScanRoot(path: string): Promise<void> {
  await invoke("add_scan_root", { path });
}

export async function removeScanRoot(path: string): Promise<RemoveRootSummary> {
  return await invoke<RemoveRootSummary>("remove_scan_root", { path });
}

export async function rescanAllRoots(): Promise<ScanSummary> {
  return await invoke<ScanSummary>("rescan_all_roots");
}
