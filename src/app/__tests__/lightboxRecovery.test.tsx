import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, expect, it, vi } from "vitest";
import { flush } from "solid-js";
import App from "../App";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";

const state = vi.hoisted(() => ({ fail: true, mounts: vi.fn(), invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => false, invoke: state.invoke }));
vi.mock("../AppGalleryView", async () => {
  const { useAppRuntime } = await import("../AppRuntimeContext");
  return { AppGalleryView: () => {
    const runtime = useAppRuntime();
    return <>
      <button onClick={() => runtime.lightbox.select(asset(1))}>First</button>
      <button onClick={() => runtime.lightbox.select(asset(2))}>Second</button>
    </>;
  } };
});
vi.mock("../../features/lightbox/LightboxModal", () => ({ default: (props: {
  model: { selected: AssetSummary | null; close: () => void }
}) => {
  state.mounts();
  if (state.fail) throw new Error("media view failed");
  return <p data-testid="healthy-media">{props.model.selected?.file_name}</p>;
} }));
function asset(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.png`,
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
  };
}
beforeEach(() => {
  state.fail = true;
  state.mounts.mockClear();
  state.invoke.mockImplementation(async (command: string, args: { assetId?: number }) => {
    if (command === "start_asset_query") return { status: "ready", session_id: 1, revision: 1, total: 2, offset: 0, items: [asset(1), asset(2)] };
    if (command === "get_asset_details") return { ...asset(args.assetId ?? 1), path: "/synthetic/image.png", size_bytes: 1, tags: [] };
    if (command === "list_tags") return { items: [], total: 0 };
    if (command === "list_scan_roots" || command === "get_startup_popular_tags") return [];
  });
});

it("retries a failed media view on reopening and preserves a healthy view on navigation", async () => {
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  try {
    render(() => <App />);
    fireEvent.click(screen.getByText("First"));
    flush();
    await screen.findByRole("alert");
    const failed = screen.getByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    flush();
    expect(failed).toBeInTheDocument();
    expect(failed.closest("[data-modal-presence]")).toHaveAttribute("aria-hidden", "true");
    await waitFor(() => expect(failed).not.toBeInTheDocument());
    state.fail = false;
    fireEvent.click(screen.getByText("First"));
    flush();
    const media = await screen.findByTestId("healthy-media");
    expect(media).toHaveTextContent("1.png");
    expect(state.mounts).toHaveBeenCalledTimes(2);
    fireEvent.click(screen.getByText("Second"));
    flush();
    await waitFor(() => expect(media).toHaveTextContent("2.png"));
    expect(screen.getByTestId("healthy-media")).toBe(media);
    expect(state.mounts).toHaveBeenCalledTimes(2);
  } finally {
    log.mockRestore();
  }
});
