import { createSignal, flush } from "solid-js";
import { render } from "@solidjs/web";
import { mockIPC } from "@tauri-apps/api/mocks";
import { GalleryGrid } from "../../src/features/gallery/GalleryGrid";
import { createSelection } from "../../src/features/gallery/createSelection";
import { createLibrary } from "../../src/library/createLibrary";
import { LanguageProvider } from "../../src/i18n/solid";
import { i18nReady } from "../../src/i18n/core";
import { UiLayerProvider } from "../../src/ui/overlays/UiLayerProvider";
import type { AssetQuerySection, AssetSummary } from "../../src/platform/tauri/contracts/assets";
import "./fixture.css";

// Grid-performance stage 10 (open topic 12): desktop timings at one million results without
// a million files. Production gallery, page cache, timeline scrubber and selection model in
// native WebKitGTK; only the Tauri transport is synthetic (12 ms per page, like `performance.tsx`).
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
const TOTAL = 1_000_000;
const MONTHS = 120; // December 2025 (newest) back to January 2016
const SPLIT = 5000; // the backend splits months larger than this (ADR 0007)
const TIMELINE_LIMIT_MS = 300;
const SELECT_ALL_LIMIT_MS = 50;
const SAMPLES = 3;

// Newest month first; sizes differ by at most one so the month starts are irregular.
const sections: AssetQuerySection[] = [];
const months: { t: number; start: number; count: number }[] = [];
for (let m = 0, start = 0; m < MONTHS; m++) {
  const count = Math.floor(TOTAL / MONTHS) + (m < TOTAL % MONTHS ? 1 : 0);
  const year = 2025 - Math.floor(m / 12);
  const t = year * 100 + (12 - (m % 12));
  months.push({ t, start, count });
  for (let offset = 0; offset < count; offset += SPLIT)
    sections.push({ t, start: start + offset, count: Math.min(SPLIT, count - offset) });
  start += count;
}
const oldest = months[months.length - 1];
const asset = (index: number): AssetSummary => ({ id: index + 1, file_name: `synthetic-${index}.png`,
  preview_path: null, kind: "image", modified_at: 0, width: 188, height: 188,
  duration_ms: null, thumb_version: "0123456789abcdef", is_favorite: false, media_group_key: null, media_group_order: null });
const calls: Record<string, number> = {};
const pageOffsets: number[] = [];
// Every start is a new session, so each timed sample starts with an empty page cache.
let session = 0;
mockIPC(async (command, payload) => {
  if (!payload || typeof payload !== "object") throw new Error(`Missing payload: ${command}`);
  calls[command] = (calls[command] ?? 0) + 1;
  if (command === "start_asset_query" || command === "get_asset_query_page") {
    const anchor = Reflect.get(payload, "anchorOffset");
    const offset = command === "start_asset_query"
      ? Math.floor(Number(anchor ?? 0) / 128) * 128 : Number(Reflect.get(payload, "offset"));
    if (command === "start_asset_query") session++;
    else pageOffsets.push(offset);
    await pause(12);
    return { status: "ready", session_id: session, revision: session, total: TOTAL, offset,
      items: Array.from({ length: Math.min(128, TOTAL - offset) }, (_, i) => asset(offset + i)) };
  }
  if (command === "get_asset_query_sections") {
    await pause(12);
    return { status: "ready", session_id: session, total: TOTAL, sections };
  }
  // A select-all must never read IDs; the call is counted and rejected.
  if (command === "get_asset_query_id_range") throw new Error("ID range read during select-all");
  throw new Error(`Unexpected synthetic command: ${command}`);
});

// WebKitGTK's `event.timeStamp` for XTest keys is not on the performance.now() clock, so
// timings start when the first (capturing) listener sees the native keydown.
const keydowns: { key: string; ctrl: boolean; time: number }[] = [];
window.addEventListener("keydown", event => keydowns.push({ key: event.key, ctrl: event.ctrlKey, time: performance.now() }), true);
let actionId = 0;
async function native(...steps: unknown[]) {
  const id = ++actionId;
  window.nativeAction = { id, steps };
  for (let i = 0; i < 300 && window.nativeDone !== id; i++) await pause(10);
  if (window.nativeDone !== id) throw new Error("native input completion");
}
const keyPress = (...keys: string[]) => [
  ...keys.map(key => ({ type: "key", key, down: true, ms: 0 })),
  ...[...keys].reverse().map(key => ({ type: "key", key, down: false, ms: 0 }))
];
/** First animation frame (performance.now()) at which `check` holds, or rejects after `ms`. */
function frameWhen(check: () => boolean, label: string, ms = 5000) {
  return new Promise<number>((resolve, reject) => {
    const deadline = performance.now() + ms;
    const frame = () => {
      if (check()) resolve(performance.now());
      else if (performance.now() > deadline) reject(new Error(label));
      else requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  });
}

await i18nReady;
let library: ReturnType<typeof createLibrary> | undefined;
let selection: ReturnType<typeof createSelection> | undefined;
let scroller: HTMLElement | undefined;
const dispose = render(() => {
  const model = createLibrary();
  const select = createSelection({
    queryEpoch: () => model.query.queryEpoch,
    identityEpoch: () => model.tags.epoch,
    getIdsRange: model.query.getIdsRangeAsync,
    sessionId: model.query.sessionId,
    total: () => model.query.navigationSnapshot().total
  });
  library = model;
  selection = select;
  const [ref, setRef] = createSignal<HTMLElement>();
  return <LanguageProvider><UiLayerProvider>
    {/* Stand-in for the bulk dock counter: the same `count` getter, no other work. */}
    <output data-testid="selection-count">{select.enabled ? String(select.count) : ""}</output>
    <section ref={el => { scroller = el; setRef(el); }} class="gallery-scroll overflow-y-scroll"
      style={{width: "1044px", height: "600px"}}>
      <GalleryGrid assets={model.query.assets} assetCount={model.query.total}
        getAssetAt={model.query.getAssetAt} tileSize={188} sections={model.query.sections}
        scroller={ref} onVirtualRangeChange={model.handleVirtualRangeChange}
        onSelect={() => {}} hasScanRoots onAddFirstFolder={() => {}} selectionModeEnabled={select.enabled}
        isSelected={select.isSelected} onBulkSelectionInteraction={select.interact} loadError={null}
        onLoadRetry={() => {}} queryEpoch={model.query.queryEpoch} pageFailureEpoch={model.query.pageFailureEpoch}/>
    </section>
  </UiLayerProvider></LanguageProvider>;
}, document.getElementById("root")!);

const slider = () => document.querySelector<HTMLElement>('[role="slider"][aria-label="Timeline"]');
/** A tile with data (not a placeholder) inside the scroller's viewport. */
function tileVisible(index: number) {
  const tile = document.querySelector<HTMLElement>(`button[data-asset-index="${index}"]`);
  if (!tile || !scroller) return false;
  const box = tile.getBoundingClientRect(), view = scroller.getBoundingClientRect();
  return box.bottom > view.top && box.top < view.bottom;
}
const percentile = (values: number[], p: number) =>
  [...values].sort((a, b) => a - b)[Math.min(values.length - 1, Math.floor(values.length * p))];
const results: Record<string, unknown> = {};
try {
  if (!library || !selection || !scroller) throw new Error("Missing mounted model");
  const query = library.query, select = selection;
  await query.refresh(); flush();
  await frameWhen(() => tileVisible(0) && !!slider(), "first page and timeline", 10000);
  if (query.total !== TOTAL) throw new Error(`total ${query.total}`);
  if (slider()!.getAttribute("aria-valuemax") !== String(MONTHS)) throw new Error("merged months");
  await pause(300);

  // 1. Timeline: native End on the focused rail → tiles of the oldest month with data.
  // Each sample starts at the top of a fresh session (cold page cache): End needs one
  // page far from the cache, Home returns to the anchor page that the refresh loaded.
  const timeline: { key: string; target: number; ms: number; pages: number[] }[] = [];
  for (let sample = 0; sample < SAMPLES; sample++) {
    if (sample > 0) {
      const previous = session;
      await query.refresh(); flush();
      await frameWhen(() => query.sessionId() === previous + 1 && query.sections.length === sections.length && tileVisible(0),
        "fresh session with sections", 10000);
      await pause(300);
    }
    for (const [key, target] of [["End", oldest.start], ["Home", 0]] as const) {
      slider()!.focus();
      const before = keydowns.length, requested = pageOffsets.length;
      await native(...keyPress(key));
      const shown = await frameWhen(() => tileVisible(target) && tileVisible(target + 4), `${key} target tiles`);
      const pressed = keydowns.slice(before).find(event => event.key === key);
      if (!pressed) throw new Error(`${key} keydown not seen`);
      timeline.push({ key, target, ms: Math.round((shown - pressed.time) * 10) / 10, pages: pageOffsets.slice(requested) });
      await pause(400);
    }
  }
  slider()!.focus();
  await native(...keyPress("End"));
  await frameWhen(() => tileVisible(oldest.start), "oldest month again");
  const oldestText = slider()!.getAttribute("aria-valuetext");
  if (oldestText !== "January 2016") throw new Error(`oldest month label ${oldestText}`);
  await native({ type: "screenshot", name: "million-oldest-month" });
  const endMs = timeline.filter(sample => sample.key === "End").map(sample => sample.ms);
  if (timeline.some(sample => sample.key === "End" && !sample.pages.length)) throw new Error("End sample served from cache");
  results.timeline = { samples: timeline, endMaxMs: Math.max(...endMs), endP50Ms: percentile(endMs, 0.5),
    limitMs: TIMELINE_LIMIT_MS, oldestStart: oldest.start, oldestText, sectionCount: sections.length, months: MONTHS };
  if (Math.max(...timeline.map(sample => sample.ms)) >= TIMELINE_LIMIT_MS) throw new Error("timeline jump over limit");

  // 2. Ctrl+A in bulk mode → counter shows 1,000,000 without reading a single ID.
  select.toggle(); flush(); await pause(200);
  const counter = document.querySelector<HTMLElement>('[data-testid="selection-count"]')!;
  const selectAll: number[] = [];
  for (let sample = 0; sample < SAMPLES; sample++) {
    if (counter.textContent !== "0") throw new Error(`counter before Ctrl+A: ${counter.textContent}`);
    let changed = 0;
    const observer = new MutationObserver(() => {
      if (!changed && counter.textContent === String(TOTAL)) changed = performance.now();
    });
    observer.observe(counter, { childList: true, characterData: true, subtree: true });
    document.querySelector<HTMLElement>(`button[data-asset-index="${oldest.start}"]`)!.focus();
    const before = keydowns.length;
    await native(...keyPress("Control_L", "a"));
    await frameWhen(() => changed > 0, "select-all counter");
    observer.disconnect();
    const pressed = keydowns.slice(before).find(event => event.ctrl && event.key.toLowerCase() === "a");
    if (!pressed) throw new Error("Ctrl+A keydown not seen");
    selectAll.push(Math.round((changed - pressed.time) * 10) / 10);
    if (!select.ranged || select.ids.size !== 0) throw new Error("select-all materialized IDs");
    await frameWhen(() => document.querySelector(`button[data-asset-index="${oldest.start}"]`)?.getAttribute("aria-pressed") === "true",
      "visible tiles selected");
    if (sample === 0) await native({ type: "screenshot", name: "million-select-all" });
    select.interact({ type: "clear" }); flush(); await pause(200);
  }
  results.selectAll = { samplesMs: selectAll, maxMs: Math.max(...selectAll), limitMs: SELECT_ALL_LIMIT_MS,
    idRangeReads: calls.get_asset_query_id_range ?? 0 };
  if ((calls.get_asset_query_id_range ?? 0) !== 0) throw new Error("ID range read");
  if (Math.max(...selectAll) >= SELECT_ALL_LIMIT_MS) throw new Error("select-all over limit");
  results.calls = { ...calls };
  dispose();
  window.scaleResult = { ok: true, results, scope: "One million synthetic results: production GalleryGrid, page cache, GalleryTimelineScrubber and createSelection in native WebKitGTK with XTest keys; mocked transport (12 ms per page and sections read), no SQLite, thumbnails decode synthetic SVG. Times run from the first capturing keydown listener to the DOM state; under host load the numbers are upper bounds." };
} catch (error) {
  window.scaleResult = { ok: false, error: String(error), stack: error instanceof Error ? error.stack : null, results, calls };
}
