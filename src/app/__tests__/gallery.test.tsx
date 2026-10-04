import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { flush } from "solid-js";
import App from "../App";
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
  await screen.findByText("7 items");
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
