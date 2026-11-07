import { cleanup, fireEvent, screen } from "@solidjs/testing-library";
import { flush } from "solid-js";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { renderSolid } from "../../../test/renderSolid";
import { UiLayerProvider } from "../../../ui/overlays/UiLayerProvider";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import type { BulkController } from "../createBulk";
import { GroupOrderModal } from "../GroupOrderModal";

const asset = (id: number): AssetSummary => ({
  id,
  file_name: `${id}.jpg`,
  preview_path: null,
  kind: "image",
  modified_at: 1,
  width: 100,
  height: 100,
  duration_ms: null,
  thumb_version: "0123456789abcdef",
  is_favorite: false,
  media_group_key: "group",
  media_group_order: null
});
const ids = Array.from({ length: 96 }, (_, index) => index + 1);
let scrollTop = 0;
beforeEach(() => {
  scrollTop = 0;
  // 960px wide lists three 312px columns; rows are 0.66 * 312 + 56 = 262px tall.
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(960);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(560);
  vi.spyOn(HTMLElement.prototype, "scrollTop", "get").mockImplementation(() => scrollTop);
  vi.spyOn(HTMLElement.prototype, "scrollTop", "set").mockImplementation((value: number) => {
    scrollTop = value;
  });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
function mount() {
  const controller = {
    orderedAssetIds: ids,
    selectedAssets: ids.map(asset),
    groupKeyDraft: "group",
    groupApplying: false,
    selectionBusy: false,
    metadataLoading: false,
    metadataFailed: false,
    onApplyGroup: vi.fn(async () => ({ status: "saved" as const }))
  } as unknown as BulkController;
  renderSolid(
    (props: { open: boolean }) => (
      <UiLayerProvider>
        <GroupOrderModal open={props.open} controller={controller} onClose={() => {}} />
      </UiLayerProvider>
    ),
    { open: true }
  );
  flush();
  return { grid: screen.getByTestId("bulk-order-grid") };
}
/** Asset IDs of `thumb://` image requests currently in the DOM. */
const thumbnailRequests = () =>
  [...document.querySelectorAll("img")]
    .map((image) => /^thumb:\/\/localhost\/(\d+)\?/.exec(image.getAttribute("src") ?? "")?.[1])
    .filter((id): id is string => id !== undefined)
    .map(Number);
const renderedTiles = () =>
  [...document.querySelectorAll<HTMLElement>("[data-sort-asset]")].map((tile) =>
    Number(tile.dataset.sortAsset)
  );

it("windows fixed-height rows and requests thumbnails only for them", () => {
  const { grid } = mount();
  // Three visible rows (560px) plus two overscan rows.
  expect(renderedTiles()).toEqual(ids.slice(0, 15));
  const content = grid.firstElementChild as HTMLElement;
  expect(content.style.height).toBe(`${32 * 262}px`);
  const rows = [...content.children].filter((row) => row.querySelector("[data-sort-asset]"));
  expect((rows[1] as HTMLElement).style.transform).toBe("translateY(262px)");
  expect(thumbnailRequests()).toEqual(ids.slice(0, 15));
  scrollTop = 262 * 20;
  fireEvent.scroll(grid);
  flush();
  expect(renderedTiles()).toEqual(ids.slice(18 * 3, 25 * 3));
  expect(thumbnailRequests()).toEqual(ids.slice(18 * 3, 25 * 3));
});

it("keeps a keyboard-moved tile in view", () => {
  mount();
  const handle = screen.getByTestId("bulk-order-handle-7");
  handle.focus();
  fireEvent.keyDown(handle, { key: "ArrowDown" });
  flush();
  // Index 6 moves to index 9 (row 3), which ends at 4 * 262 = 1048px.
  expect(scrollTop).toBe(1048 - 560);
  expect(renderedTiles().indexOf(7)).toBeGreaterThan(-1);
});
