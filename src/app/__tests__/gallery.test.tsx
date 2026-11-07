import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { flush } from "solid-js";
import App from "../App";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => false,
  invoke: native.invoke,
  convertFileSrc: (value: string) => value
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it("starts queries after mount, submits current controls once and retains drafts across Settings", async () => {
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "start_asset_query")
      return { status: "ready", session_id: 1, revision: 1, total: 7, offset: 0, items: [] };
    if (command === "list_tags") return { items: [], total: 0 };
    if (command === "list_scan_roots") return [];
  });
  render(() => <App />);
  // The count appears in the top bar and in the end-of-results footer.
  await screen.findAllByText("7 items");
  expect(
    native.invoke.mock.calls.filter(([command]) => command === "start_asset_query")
  ).toHaveLength(1);
  const input = screen.getByRole("combobox");
  fireEvent.input(input, { target: { value: '"tags"' } });
  flush();
  fireEvent.click(screen.getByRole("radio", { name: "Video" }));
  flush();
  await waitFor(() =>
    expect(native.invoke).toHaveBeenCalledWith(
      "start_asset_query",
      expect.objectContaining({ tagsAnd: ["tags"], kind: "video" })
    )
  );
  expect(
    native.invoke.mock.calls.filter(([command]) => command === "start_asset_query")
  ).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
  flush();
  await screen.findByRole("heading", { level: 1 });
  fireEvent.keyDown(window, { key: "Escape" });
  flush();
  expect(screen.getByRole("combobox")).toHaveValue('"tags"');
  expect(
    native.invoke.mock.calls.filter(([command]) => command === "start_asset_query")
  ).toHaveLength(2);
});

it.each([0, 7])("shows capacity failure with %s previous rows and recovers by narrowing the query", async (initialTotal) => {
  let starts = 0;
  const assets: AssetSummary[] = Array.from({ length: initialTotal }, (_, index) => ({
    id: index + 1,
    file_name: `synthetic-${index}.png`,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_version: "0123456789abcdef",
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  }));
  native.invoke.mockImplementation(async (command: string) => {
    if (command === "start_asset_query") {
      starts++;
      if (starts === 2) throw "Query exceeds the retained session memory limit. Narrow your search and try again.";
      return { status: "ready", session_id: starts, revision: 1, total: starts === 1 ? initialTotal : 1, offset: 0, items: starts === 1 ? assets : assets.slice(0, 1) };
    }
    if (command === "list_tags") return { items: [], total: 0 };
    if (command === "list_scan_roots") return [];
  });
  render(() => <App />);
  await screen.findByText(`${initialTotal} items`, { selector: ".workspace-count" });
  const input = screen.getByRole("combobox");
  fireEvent.input(input, { target: { value: '"broad"' } });
  flush();
  fireEvent.keyDown(input, { key: "Enter" });
  flush();
  await screen.findByTestId("gallery-load-error");
  expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
  expect(screen.getByText(`${initialTotal} items`, { selector: ".workspace-count" })).toBeVisible();
  expect(starts).toBe(2);
  fireEvent.input(input, { target: { value: '"small"' } });
  flush();
  fireEvent.keyDown(input, { key: "Enter" });
  flush();
  await screen.findByText("1 items");
  expect(screen.queryByTestId("gallery-load-error")).toBeNull();
  expect(native.invoke).toHaveBeenCalledWith("start_asset_query", expect.objectContaining({ tagsAnd: ["small"] }));
  expect(starts).toBe(3);
});
