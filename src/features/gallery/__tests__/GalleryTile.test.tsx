import { cleanup } from "@solidjs/testing-library";
import { afterEach, describe, expect, it } from "vitest";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import { renderSolid } from "../../../test/renderSolid";
import { GalleryTile } from "../GalleryTile";

afterEach(() => cleanup());

const summary = (overrides: Partial<AssetSummary>): AssetSummary => ({
  id: 7,
  file_name: "clip.mp4",
  preview_path: null,
  kind: "image",
  modified_at: 0,
  width: 320,
  height: 240,
  duration_ms: null,
  thumb_version: "0123456789abcdef",
  is_favorite: false,
  media_group_key: null,
  media_group_order: null,
  ...overrides
});
const tile = (asset: AssetSummary) =>
  renderSolid(GalleryTile, {
    asset,
    index: 0,
    selected: false,
    thumbnailsPaused: false,
    prefetch: false,
    onClick: () => {}
  });

describe("GalleryTile accessible name", () => {
  // Videos and GIFs carry their full source path in `preview_path`; the tile's text
  // alternative must not read that path aloud (grid-performance open topic 7).
  it.each([
    ["video", "clip.mp4", "/srv/media/holiday/clip.mp4"],
    ["gif", "loop.gif", "/srv/media/holiday/loop.gif"],
    ["image", "photo.jpg", null]
  ] as const)("uses the file name for a %s tile", (kind, fileName, previewPath) => {
    const view = tile(summary({ kind, file_name: fileName, preview_path: previewPath }));
    const image = view.container.querySelector("img");
    expect(image?.getAttribute("alt")).toBe(fileName);
    expect(view.container.innerHTML).not.toContain("/srv/media");
  });
});
