import "./fixture.css";

// Measures where native WebKitGTK scrolling, layout, painting and hit testing stop
// being exact for a tall spacer. Plain DOM; no application models are involved.
// Screenshots are checked afterwards by pixel (see docs/development/testing.md).
declare global {
  interface Window {
    scaleResult?: unknown;
    nativeAction?: { id: number; steps: unknown[] };
    nativeDone?: number;
  }
}
const pause = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const frames = (count = 2) =>
  new Promise<void>((resolve) => {
    let left = count;
    const tick = () => (--left > 0 ? requestAnimationFrame(tick) : resolve());
    requestAnimationFrame(tick);
  });
let actionId = 0;
async function native(...steps: unknown[]) {
  const id = ++actionId;
  window.nativeAction = { id, steps };
  for (let i = 0; i < 200 && window.nativeDone !== id; i++) await pause(20);
  if (window.nativeDone !== id) throw new Error("native input completion");
  await frames(2);
}
const VIEWPORT = 600;
const FOOTER = 136;
const root = document.getElementById("root") as HTMLElement;
root.innerHTML = `
<style>
  .probe-marker { position: absolute; left: 0; top: 0; width: 300px; height: 180px; contain: strict; }
  .gallery-scroll[data-probe-thumb="color"] { scrollbar-color: transparent transparent; }
  .gallery-scroll[data-probe-thumb="width"] { scrollbar-width: none; }
</style>
<section id="scroller" class="gallery-scroll" style="width:1044px;height:${VIEWPORT}px;overflow-x:hidden;overflow-y:auto;position:relative">
  <div id="content" style="position:relative;overflow-anchor:none">
    <div id="mid" class="probe-marker" style="background:#2a7"></div>
    <div id="end" class="probe-marker" style="background:#c33"></div>
  </div>
  <div style="height:${FOOTER}px;background:#335">footer</div>
</section>`;
const scroller = document.getElementById("scroller") as HTMLElement;
const content = document.getElementById("content") as HTMLElement;
const markers = {
  mid: document.getElementById("mid") as HTMLElement,
  end: document.getElementById("end") as HTMLElement
};

function probeMarker(name: "mid" | "end", y: number, target: number) {
  scroller.scrollTop = target;
  const readBack = scroller.scrollTop;
  const box = scroller.getBoundingClientRect();
  const rect = markers[name].getBoundingClientRect();
  const relativeTop = rect.top - box.top - scroller.clientTop;
  const hitY = Math.min(box.top + VIEWPORT - 2, Math.max(box.top + 2, rect.top + 90));
  const hit = document.elementFromPoint(box.left + 150, hitY);
  return {
    name,
    y,
    target,
    readBack,
    scrollError: readBack - target,
    relativeTop,
    layoutError: relativeTop - (y - readBack),
    hit: hit?.id ?? hit?.tagName ?? null
  };
}

// 2^23 and 2^24 px bracket float32 precision at device scale 2 and 1.
const HEIGHTS = [
  4_000_000, 6_000_000, 8_000_000, 8_388_000, 8_400_000, 9_000_000, 12_000_000, 16_000_000,
  16_777_000, 16_800_000, 17_000_000, 20_000_000, 33_500_000
];
const results: unknown[] = [];
try {
  await frames(3);
  for (const height of HEIGHTS) {
    content.style.height = `${height}px`;
    // Odd offsets expose float32 rounding above 2^24 px.
    const midY = Math.floor(height / 2) + 123;
    const endY = height - 401;
    markers.mid.style.transform = `translate(0, ${midY}px)`;
    markers.end.style.transform = `translate(0, ${endY}px)`;
    await frames(2);
    const contentHeight = content.getBoundingClientRect().height;
    const scrollHeight = scroller.scrollHeight;
    // A huge finite write must clamp; Number.MAX_SAFE_INTEGER resets to 0 in WebKitGTK.
    scroller.scrollTop = height * 2;
    const maxTop = scroller.scrollTop;
    await frames(1);
    const probes = [
      probeMarker("mid", midY, midY - 101),
      probeMarker("mid", midY, midY - 100.5),
      probeMarker("end", endY, endY - 101),
      probeMarker("end", endY, height + FOOTER - VIEWPORT)
    ];
    scroller.scrollTop = endY - 300;
    await frames(2);
    const beforeWheel = scroller.scrollTop;
    const box = scroller.getBoundingClientRect();
    await native(
      { type: "move", x: box.left + 400, y: box.top + 300 },
      { type: "button", button: 5, down: true },
      { type: "button", button: 5, down: false },
      { type: "pause", ms: 350 }
    );
    const wheelDelta = scroller.scrollTop - beforeWheel;
    // The end marker is expected at viewport y 200..380, x 0..300 (red).
    scroller.scrollTop = endY - 200;
    await frames(3);
    const paintScrollTop = scroller.scrollTop;
    const screenshot = `height-probe-paint-${Math.round(height / 1000)}k`;
    await native({ type: "screenshot", name: screenshot });
    const exactLayout =
      Math.abs(contentHeight - height) < 1 &&
      Math.abs(scrollHeight - (height + FOOTER)) <= 1 &&
      Math.abs(maxTop - (height + FOOTER - VIEWPORT)) <= 1 &&
      probes.every(
        (probe) =>
          Math.abs(probe.scrollError) <= 0.5 &&
          Math.abs(probe.layoutError) < 0.5 &&
          probe.hit === probe.name
      ) &&
      wheelDelta > 0 &&
      wheelDelta < VIEWPORT;
    results.push({
      height,
      exactLayout,
      contentHeight,
      scrollHeight,
      maxTop,
      wheelDelta,
      paintScrollTop,
      screenshot,
      probes
    });
  }
  scroller.scrollTop = Number.MAX_SAFE_INTEGER;
  const maxSafeWrite = scroller.scrollTop;
  // Native thumb hiding: does the gutter width change, and does the gutter still scroll?
  markers.mid.style.transform = "translate(0, 0)";
  markers.end.style.transform = "translate(0, 0)";
  content.style.height = "16000000px";
  await frames(2);
  const thumb: unknown[] = [];
  for (const mode of ["visible", "color", "width"]) {
    if (mode === "visible") delete scroller.dataset.probeThumb;
    else scroller.dataset.probeThumb = mode;
    scroller.scrollTop = 8_000_000;
    await frames(3);
    const clientWidth = scroller.clientWidth;
    const box = scroller.getBoundingClientRect();
    await native({ type: "screenshot", name: `height-probe-thumb-${mode}` });
    const before = scroller.scrollTop;
    // Press low in the gutter, well away from the thumb at the middle.
    await native(
      { type: "move", x: box.right - 4, y: box.top + 520 },
      { type: "button", button: 1, down: true },
      { type: "button", button: 1, down: false },
      { type: "pause", ms: 250 }
    );
    thumb.push({ mode, clientWidth, before, after: scroller.scrollTop, moved: scroller.scrollTop - before });
  }
  window.scaleResult = {
    ok: true,
    devicePixelRatio: window.devicePixelRatio,
    results,
    maxSafeWrite,
    thumb,
    scope:
      "Native WebKitGTK scroll container with production .gallery-scroll CSS, plain DOM spacer, 2D-translated contain:strict markers, XTest wheel and scrollbar input on private Xvfb."
  };
} catch (error) {
  window.scaleResult = { ok: false, error: String(error), results };
}
