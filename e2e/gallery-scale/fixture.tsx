import { createSignal, flush } from "solid-js";
import { render } from "@solidjs/web";
import { GalleryGrid } from "../../src/features/gallery/GalleryGrid";
import { createSelection } from "../../src/features/gallery/createSelection";
import { LanguageProvider } from "../../src/i18n/solid";
import { i18nReady } from "../../src/i18n/core";
import { ThumbnailStore } from "../../src/library/thumbnails/thumbnailStore";
import { UiLayerProvider } from "../../src/ui/overlays/UiLayerProvider";
import type { AssetSummary } from "../../src/platform/tauri/contracts/assets";
import type { LibraryRange } from "../../src/library/createLibrary";
import "./fixture.css";

declare global {
  interface Window {
    scaleResult?: unknown;
    nativeAction?: { id: number; steps: unknown[] };
    nativeDone?: number;
  }
}
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function until(check: () => boolean, label: string) {
  for (let i = 0; i < 100; i++) {
    flush();
    if (check()) return;
    await pause(30);
  }
  throw new Error(label);
}
const settle = async () => { flush(); await pause(200); flush(); };
function assert(ok: unknown, label: string): asserts ok { if (!ok) throw new Error(label); }
let actionId = 0;
async function native(...steps: unknown[]) {
  const id = ++actionId;
  window.nativeAction = { id, steps };
  await until(() => window.nativeDone === id, "native input completion");
  await settle();
}
const ranges: LibraryRange[] = [];
const selectionReads: number[][] = [];
let controls: {
  configure: (count: number, tile: number, more?: boolean) => void;
  selection: ReturnType<typeof createSelection>;
} | undefined;
let scrollElement: HTMLElement | undefined;
function scroller() { assert(scrollElement, "mounted scroller"); return scrollElement; }
function selection() { assert(controls, "mounted selection"); return controls.selection; }
function configure(count: number, tile: number, more = false) { assert(controls, "mounted controls"); controls.configure(count, tile, more); }
let selected: number | null = null;
let reachEndCalls = 0;
const asset = (index: number): AssetSummary => ({ id: index + 1, file_name: `synthetic-${index}.png`,
  preview_path: null, kind: "image", modified_at: 0, width: 100, height: 100,
  duration_ms: null, thumb_path: null, is_favorite: false, media_group_key: null, media_group_order: null });
await i18nReady;
const dispose = render(() => {
  const [count, setCount] = createSignal(1_000_000);
  const [tile, setTile] = createSignal(188);
  const [more, setMore] = createSignal(false);
  const [ref, setRef] = createSignal<HTMLElement>();
  const selection = createSelection({ queryEpoch: () => 1, identityEpoch: () => 1,
    getIdsRange: async (start, end) => {
      selectionReads.push([start, end]);
      return Array.from({length: end - start + 1}, (_, i) => start + i + 1);
    }
  });
  controls = { selection, configure: (nextCount, nextTile, nextMore = false) => { setCount(nextCount); setTile(nextTile); setMore(nextMore); } };
  return <LanguageProvider><UiLayerProvider>
    <section ref={element => { scrollElement = element; setRef(element); }} class="gallery-scroll h-full overflow-x-hidden overflow-y-auto"
      style={{ width: "1044px", height: "600px", "overflow-y": "scroll", "overflow-x": "hidden" }}>
      <div class="workspace-body">
        <GalleryGrid assets={[]} assetCount={count()} tileSize={tile()}
          getAssetAt={index => index < count() ? asset(index) : undefined}
          hasMore={more()} isLoading={false} isGeneratingThumbnails={false} pendingThumbnailCount={0}
          thumbnailStore={new ThumbnailStore()} scroller={ref} onReachEnd={() => { reachEndCalls++; }}
          onVirtualRangeChange={range => { if (range) ranges.push({...range}); }}
          onSelect={value => { selected = value.id; }} hasScanRoots={true} onAddFirstFolder={() => {}}
          selectionModeEnabled={selection.enabled} selectedAssetIds={selection.ids} onBulkSelectionInteraction={selection.interact}
          loadError={null} onLoadRetry={() => {}} queryEpoch={1} pageFailureEpoch={0}/>
      </div>
    </section>
  </UiLayerProvider></LanguageProvider>;
}, document.getElementById("root")!);
const grid = () => document.querySelector<HTMLElement>('[data-testid="gallery-grid"] > div')!;
const tile = (index: number) => document.querySelector<HTMLButtonElement>(`button[data-asset-index="${index}"]`);
function snapshot() {
  const tiles = Array.from(document.querySelectorAll<HTMLButtonElement>('button[data-asset-index]'));
  const viewport = scroller().getBoundingClientRect();
  const visible = tiles.filter(el => { const r = el.getBoundingClientRect(); return r.bottom > viewport.top && r.top < viewport.bottom; });
  return { height: grid().getBoundingClientRect().height, scrollHeight: scroller().scrollHeight, scrollTop: scroller().scrollTop,
    mounted: tiles.length, range: ranges.at(-1), visible: visible.map(el => Number(el.dataset.assetIndex)),
    columns: Math.floor((grid().clientWidth + 12) / (tiles[0].getBoundingClientRect().width + 12)), selected };
}
async function width(value: number) {
  for (let i = 0; i < 3; i++) {
    scroller().style.width = `${scroller().getBoundingClientRect().width + value - grid().clientWidth}px`;
    await settle();
  }
}
function point(index: number, corner = false) {
  const rect = tile(index)!.getBoundingClientRect();
  return { x: rect.left + (corner ? 3 : rect.width / 2), y: rect.top + (corner ? 3 : rect.height / 2) };
}
async function click(index: number, shift = false) {
  const p = point(index);
  await native({type: "move", ...p}, ...(shift ? [{type: "key", key: "Shift_L", down: true}] : []),
    {type: "button", button: 1, down: true}, {type: "button", button: 1, down: false},
    ...(shift ? [{type: "key", key: "Shift_L", down: false}] : []));
}
const results: unknown[] = [];
try {
  await settle();
  for (const [name, count, size, w, more] of [
    ["million-five-columns", 1_000_000, 188, 988, false],
    ["million-narrow", 1_000_000, 188, 228, false],
    ["million-single-column", 1_000_000, 340, 588, false],
    ["million-smaller-tiles", 1_000_000, 140, 988, true],
    ["million-wide", 1_000_000, 188, 1588, false],
    ["small-control", 100, 188, 988, false]
  ] as const) {
    configure(count, size, more); await settle(); await width(w);
    for (let n = 0; n < 3; n++) { scroller().scrollTop = scroller().scrollHeight; await settle(); }
    const state = snapshot();
    assert(state.visible.includes(count - 1), `${name}: last visible`);
    assert(state.height <= 4_000_000, `${name}: physical bound`);
    assert(state.mounted < 120, `${name}: mounted bound`);
    assert(state.range?.endIndex === count - 1, `${name}: final page demand`);
    await click(count - 1);
    assert(selected === count, `${name}: native last click`);
    results.push({name, ...state, selected, reachEndCalls});
  }
  configure(1_000_000, 188); await settle(); await width(988);
  scroller().scrollTop = 2_000_000; await settle();
  const anchor = snapshot().range!.visibleStartIndex;
  await width(588);
  assert(snapshot().range!.visibleStartIndex === Math.floor(anchor / 3) * 3, "resize anchor");
  configure(1_000_000, 340); await settle();
  assert(snapshot().range!.visibleStartIndex === Math.floor(anchor / 3) * 3, "tile anchor");
  const beforeHeight = snapshot().range!.visibleStartIndex;
  scroller().style.height = "450px"; await settle();
  assert(snapshot().range!.visibleStartIndex === beforeHeight, "viewport height anchor");
  scroller().style.height = "600px"; await settle();
  results.push({name: "resize-and-tile-anchor", anchor, ...snapshot()});
  configure(1_000_000, 188); await settle(); await width(988);
  scroller().scrollTop = 0; await settle();
  // Drag the actual WebKit scrollbar thumb from the start to the end.
  const box = scroller().getBoundingClientRect();
  const x = box.right - 7;
  await native({type: "move", x, y: 10}, {type: "button", button: 1, down: true},
    {type: "move", x, y: 590}, {type: "button", button: 1, down: false});
  assert(snapshot().visible.includes(999_999), "native scrollbar reaches final result");
  await native({type: "screenshot", name: "last-result"});
  results.push({name: "native-scrollbar", ...snapshot()});
  // Move away from the footer and use genuine X11 wheel events.
  scroller().scrollTop = 2_000_000; await settle();
  const before = snapshot();
  await native({type: "move", x: 400, y: 300}, {type: "button", button: 5, down: true}, {type: "button", button: 5, down: false});
  await until(() => snapshot().scrollTop !== before.scrollTop, "native wheel changes scroll");
  const after = snapshot();
  assert(after.range!.visibleStartIndex >= before.range!.visibleStartIndex, "wheel direction");
  assert(after.range!.visibleStartIndex - before.range!.visibleStartIndex <= 5, "wheel stays local");
  results.push({name: "native-wheel", before, after});
  selection().toggle(); await settle();
  const first = snapshot().range!.visibleStartIndex + 5;
  await click(first); await click(first + 2, true);
  assert([...selection().ids].sort().join() === [first + 1, first + 2, first + 3].sort().join(), "shift range global IDs");
  selection().clear(); await settle();
  const start = point(first, true), end = point(first + 1);
  await native({type: "move", ...start}, {type: "button", button: 1, down: true}, {type: "pause", ms: 200},
    {type: "move", ...end}, {type: "button", button: 1, down: false});
  await until(() => !selection().busy, "rectangle completes");
  assert(selection().ids.has(first + 1) && selection().ids.has(first + 2) && selection().ids.size === 2, "rectangle global IDs");
  results.push({name: "native-selection", ids: [...selection().ids], reads: selectionReads, ...snapshot()});
  await native({type: "screenshot", name: "rectangle-selection"});
  const calls = ranges.length;
  dispose();
  scroller().dispatchEvent(new Event("scroll")); window.dispatchEvent(new Event("resize")); await settle();
  assert(ranges.length === calls && document.querySelectorAll('[data-asset-index]').length === 0, "disposed range and DOM");
  window.scaleResult = { ok: true, results, scope: "Real GalleryGrid, virtual-core, selection and CSS in native WebKitGTK. Lazy synthetic summaries and range reads, no Tauri/SQLite/media or timing comparison." };
} catch (error) {
  window.scaleResult = { ok: false, error: String(error), stack: error instanceof Error ? error.stack : null, results, state: snapshot() };
}
