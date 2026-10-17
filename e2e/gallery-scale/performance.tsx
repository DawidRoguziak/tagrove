import { createSignal, flush } from "solid-js";
import { render } from "@solidjs/web";
import { mockIPC } from "@tauri-apps/api/mocks";
import { Channel } from "@tauri-apps/api/core";
import { GalleryGrid } from "../../src/features/gallery/GalleryGrid";
import { createLibrary } from "../../src/library/createLibrary";
import { LanguageProvider } from "../../src/i18n/solid";
import { i18nReady } from "../../src/i18n/core";
import { UiLayerProvider } from "../../src/ui/overlays/UiLayerProvider";
import type { AssetSummary } from "../../src/platform/tauri/contracts/assets";
import "./fixture.css";

// Production models and components; only transport and file URLs are synthetic.
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
const total = 26_000;
const requests: { command: string; offset?: number; ids?: number[]; time: number }[] = [];
const asset = (index: number): AssetSummary => ({ id: index + 1, file_name: `synthetic-${index}.png`,
  preview_path: null, kind: "image", modified_at: 0, width: 188, height: 188,
  duration_ms: null, thumb_path: null, is_favorite: false, media_group_key: null, media_group_order: null });
const image = (id: number) => `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="188" height="188"><rect width="188" height="188" fill="hsl(${id % 360},60%,35%)"/><circle cx="94" cy="78" r="48" fill="hsl(${(id + 80) % 360},60%,65%)"/><text x="12" y="172" fill="white" font-size="22">${id}</text></svg>`)}`;
mockIPC(async (command, payload) => {
  if (!payload || typeof payload !== "object") throw new Error(`Missing payload: ${command}`);
  if (command === "start_asset_query" || command === "get_asset_query_page") {
    const offset = command === "start_asset_query" ? 0 : Number(Reflect.get(payload, "offset"));
    requests.push({ command, offset, time: performance.now() });
    await pause(12);
    return { status: "ready", session_id: 1, revision: 1, total, offset,
      items: Array.from({ length: Math.min(128, total - offset) }, (_, i) => asset(offset + i)) };
  }
  if (command === "ensure_thumbnails") {
    const ids = [...Reflect.get(payload, "visibleIds"), ...Reflect.get(payload, "prefetchIds")];
    const channel = Reflect.get(payload, "onEvent");
    if (!(channel instanceof Channel)) throw new Error("Missing thumbnail channel");
    requests.push({ command, ids, time: performance.now() });
    await pause(20);
    for (const id of ids) channel.onmessage({ event: "ready", data: { asset_id: id, thumb_path: image(id) } });
    return;
  }
  throw new Error(`Unexpected synthetic command: ${command}`);
});
Reflect.set(Reflect.get(window, "__TAURI_INTERNALS__"), "convertFileSrc", (path: string) => path);
await i18nReady;
let library: ReturnType<typeof createLibrary> | undefined;
let scroller: HTMLElement | undefined;
const dispose = render(() => {
  const model = createLibrary();
  // Saved pre-pause sources expose only the sequential query callback.
  library = model;
  const [ref, setRef] = createSignal<HTMLElement>();
  return <LanguageProvider><UiLayerProvider>
    <section ref={el => { scroller = el; setRef(el); }} class="gallery-scroll overflow-y-scroll"
      style={{width: "1044px", height: "600px"}}>
      <GalleryGrid assets={model.query.assets} assetCount={model.query.total}
        getAssetAt={model.query.getAssetAt} tileSize={188} hasMore={model.query.offset < model.query.total}
        isLoading={model.query.loading} isGeneratingThumbnails={model.thumbnails.isGeneratingPage}
        pendingThumbnailCount={model.thumbnails.pendingPageSize} thumbnailStore={model.thumbnails.thumbnailStore}
        scroller={ref} onReachEnd={model.handleGalleryReachEnd ?? model.query.handleReachEnd} onVirtualRangeChange={model.handleVirtualRangeChange}
        onSelect={() => {}} hasScanRoots onAddFirstFolder={() => {}} selectionModeEnabled={false}
        selectedAssetIds={new Set()} onBulkSelectionInteraction={() => {}} loadError={null}
        onLoadRetry={() => {}} queryEpoch={model.query.queryEpoch} pageFailureEpoch={model.query.pageFailureEpoch}/>
    </section>
  </UiLayerProvider></LanguageProvider>;
}, document.getElementById("root")!);
const results = [];
const moderateFrames: { loaded: number; dimmed: number; paused: boolean; minOpacity: number }[] = [];
let actionId = 0;
try {
  if (!library || !scroller) throw new Error("Missing mounted model");
  const element = scroller;
  // Each move stays below the fast-scroll thresholds: 240px/60ms in a 600px viewport.
  await library.query.refresh(); flush(); await pause(700);
  let samplingModerate = true;
  const sampleModerate = () => {
    if (!samplingModerate) return;
    const loaded = Array.from(element.querySelectorAll("img")).filter(img =>
      img.src.startsWith("data:image/svg+xml") && img.complete && img.naturalWidth &&
      img.getBoundingClientRect().bottom > 0 && img.getBoundingClientRect().top < 600);
    const opacities = loaded.map(img => Number(getComputedStyle(img).opacity));
    moderateFrames.push({ loaded: loaded.length, dimmed: opacities.filter(value => value < 0.95).length,
      minOpacity: Math.min(1, ...opacities), paused: !!element.querySelector("[data-scroll-paused]:not([data-scroll-paused=false])") });
    requestAnimationFrame(sampleModerate);
  };
  requestAnimationFrame(sampleModerate);
  for (let step = 0; step < 40; step++) {
    element.scrollTop += 240;
    if (step === 20) window.nativeAction = { id: ++actionId, steps: [{type: "screenshot", name: "moderate-scroll"}] };
    await pause(60);
  }
  samplingModerate = false;
  await pause(400);
  if (window.nativeDone !== actionId) throw new Error("Missing moderate-scroll screenshot");
  for (let pass = 0; pass < 3; pass++) {
    element.scrollTop = 0;
    await library.query.refresh(); flush(); await pause(700);
    requests.length = 0;
    const frames: number[] = [];
    let lastFrame = performance.now(), lastScroll = 0, scrolls = 0, pausedFrames = 0;
    let invalidPausedFrames = 0, loadedAt = 0, visibleAt = 0;
    const scroll = () => { lastScroll = performance.now(); scrolls++; loadedAt = 0; visibleAt = 0; };
    element.addEventListener("scroll", scroll);
    let running = true;
    const frame = (time: number) => {
      if (!running) return;
      if (lastScroll && time - lastScroll < 150) frames.push(time - lastFrame);
      lastFrame = time;
      const gallery = document.querySelector('[data-testid="gallery-grid"]');
      const imgs = Array.from(element.querySelectorAll("img"));
      if (gallery?.hasAttribute("data-scroll-paused") && gallery.getAttribute("data-scroll-paused") !== "false") {
        pausedFrames++;
        if (imgs.length || element.querySelector(".animate-pulse, .animate-spin")) invalidPausedFrames++;
      }
      if (lastScroll && !loadedAt) {
        const ready = imgs.filter(img => img.src.startsWith("data:image/svg+xml") && img.complete && img.naturalWidth && img.getBoundingClientRect().bottom > 0 && img.getBoundingClientRect().top < 600);
        if (ready.length) loadedAt = performance.now();
      }
      if (loadedAt && !visibleAt && imgs.some(img => img.src.startsWith("data:image/svg+xml") && img.complete && img.naturalWidth && Number(getComputedStyle(img).opacity) > 0.95 && img.getBoundingClientRect().bottom > 0 && img.getBoundingClientRect().top < 600)) visibleAt = performance.now();
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
    const x = element.getBoundingClientRect().right - 7;
    const id = ++actionId;
    window.nativeAction = { id, steps: [{ type: "move", x, y: 10 }, { type: "button", button: 1, down: true },
      ...Array.from({length: 60}, (_, i) => ({ type: "move", x, y: 10 + (i + 1) * 580 / 60, ms: 20 })),
      { type: "button", button: 1, down: false }] };
    for (let i = 0; i < 500 && window.nativeDone !== id; i++) await pause(10);
    await pause(900);
    running = false;
    element.removeEventListener("scroll", scroll);
    if (!element.querySelector('button[data-asset-index="25999"]')) throw new Error("Drag did not reach final tile");
    if (window.nativeDone !== id || !scrolls || !loadedAt || !visibleAt || invalidPausedFrames) throw new Error("Incomplete drag or invalid paused DOM");
    const sorted = [...frames].sort((a, b) => a - b);
    results.push({ pass, scrolls, frameCount: frames.length, frameP50: sorted[Math.floor(sorted.length * .5)],
      frameP95: sorted[Math.floor(sorted.length * .95)], frameMax: Math.max(...frames), over25ms: frames.filter(ms => ms > 25).length,
      pageRequests: requests.filter(r => r.command === "get_asset_query_page").length,
      thumbnailRequests: requests.filter(r => r.command === "ensure_thumbnails").length,
      thumbnailIds: requests.flatMap(r => r.ids ?? []).length, loadedAfterStopMs: loadedAt - lastScroll,
      visibleAfterStopMs: visibleAt - lastScroll, pausedFrames, invalidPausedFrames,
      finalScrollTop: element.scrollTop, requests, frames });
    // Keep each pass's request array independent.
    results[results.length - 1].requests = [...requests];
  }
  const screenshotId = ++actionId;
  window.nativeAction = { id: screenshotId, steps: [{ type: "screenshot", name: "performance-thumbnails" }] };
  for (let i = 0; i < 100 && window.nativeDone !== screenshotId; i++) await pause(10);
  if (window.nativeDone !== screenshotId) throw new Error("Missing screenshot completion");
  dispose();
  window.scaleResult = { ok: true, results, moderateFrames, scope: "Native WebKitGTK scrollbar drag; production gallery, page cache and thumbnail queue; 26000 synthetic summaries and SVG thumbnails; mocked transport delays 12ms/page, 20ms/batch. Synthetic SVG decoding only; no SQLite, real media or native IPC benchmark." };
} catch (error) {
  window.scaleResult = { ok: false, error: String(error), results, moderateFrames };
}
