import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import { cleanup, fireEvent as domEvent, screen, renderHook } from "@solidjs/testing-library";
import { settle as act } from "../../../test/settle";
import { renderSolid } from "../../../test/renderSolid";
import { createSignal, createEffect, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UiLayerProvider, createUiLayer } from "../../../ui/overlays/UiLayerProvider";
import type { BulkSelectionHandler } from "../selection";
import { createSelection } from "../createSelection";
import { createGridHandlers } from "../createGridHandlers";

class TestPointerEvent extends MouseEvent {
  pointerId: number;
  isPrimary: boolean;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.isPrimary = init.isPrimary ?? true;
  }
}
const asset = {
  id: 1,
  file_name: "one.png",
  preview_path: null,
  kind: "image" as const,
  modified_at: 0,
  width: 100,
  height: 100,
  duration_ms: null,
  thumb_version: "0123456789abcdef",
  is_favorite: false,
  media_group_key: null,
  media_group_order: null
};
let frame: FrameRequestCallback | undefined;
let clock = 0;
const captured = new Set<number>();
function Harness(props: {
  interaction: BulkSelectionHandler;
  epoch?: number;
  offset?: number;
  size?: number;
  enabled?: boolean;
  shift?: () => number;
  scrollBy?: (delta: number) => void;
  onSelect?: (selected: AssetSummary) => void;
}) {
  const [gridRef, setGrid] = createSignal<HTMLDivElement>();
  const [scroller, setScroller] = createSignal<HTMLDivElement>();
  const [gallery, setGallery] = createSignal<HTMLElement>();
  const handlers = createGridHandlers({
    assets: [asset],
    assetCount: 3000,
    get queryEpoch() {
      return props.epoch ?? 0;
    },
    get gridOffset() {
      return props.offset ?? 0;
    },
    gridRef,
    get scrollShift() { return props.shift?.() ?? 0; },
    scrollBy: (delta) => { if (props.scrollBy) props.scrollBy(delta); else if (scroller()) scroller()!.scrollTop += delta; },
    scrollContainerRef: scroller,
    columnCount: 3,
    get tilePixelSize() {
      return props.size ?? 100;
    },
    tileGap: 10,
    get selectionModeEnabled() {
      return props.enabled ?? true;
    },
    onSelect: (selected) => props.onSelect?.(selected),
    onBulkSelectionInteraction: (interaction) => props.interaction(interaction)
  });
  createEffect(gallery, (node) => {
    node?.addEventListener("click", handlers.handleClickCapture, true);
    return () => node?.removeEventListener("click", handlers.handleClickCapture, true);
  });
  return (
    <div ref={setScroller} data-testid="scroller">
      <section
        ref={setGallery}
        data-testid="gallery"
        onPointerDown={handlers.handlePointerDown}
        onPointerMove={handlers.handlePointerMove}
        onPointerUp={handlers.handlePointerUp}
        onPointerCancel={handlers.handlePointerCancel}
        onLostPointerCapture={handlers.handleLostPointerCapture}
      >
        <div ref={setGrid} data-testid="grid">
          <button data-asset-id="1" data-asset-index="0" onClick={handlers.handleTileClick}>
            tile
          </button>
        </div>
        <button>control</button>
        <output>{JSON.stringify(handlers.preview)}</output>
      </section>
    </div>
  );
}
function bounds(left: number, top: number, width: number, height: number): DOMRect {
  return new DOMRect(left, top, width, height);
}
function layout() {
  const scroller = screen.getByTestId("scroller");
  Object.defineProperties(scroller, { clientWidth: { value: 340 }, clientHeight: { value: 400 } });
  vi.spyOn(scroller, "getBoundingClientRect").mockImplementation(() => bounds(0, 0, 340, 400));
  vi.spyOn(screen.getByTestId("grid"), "getBoundingClientRect").mockImplementation(() =>
    bounds(10, 10 - scroller.scrollTop, 320, 110000)
  );
  vi.spyOn(screen.getByTestId("gallery"), "getBoundingClientRect").mockImplementation(() =>
    bounds(0, 0, 340, 110000)
  );
  return scroller;
}
function down(x = 15, y = 15, modifiers = {}) {
  fireEvent.pointerDown(screen.getByText("tile"), { clientX: x, clientY: y, ...modifiers });
}
function move(x: number, y: number) {
  fireEvent.pointerMove(screen.getByTestId("gallery"), { clientX: x, clientY: y });
}
function up(x: number, y: number) {
  fireEvent.pointerUp(screen.getByTestId("gallery"), { clientX: x, clientY: y });
}
function tick() {
  act(() => {
    clock += 16;
    frame?.(clock);
  });
}
function preview() {
  return JSON.parse(screen.getByRole("status").textContent ?? "null");
}

beforeEach(() => {
  captured.clear();
  clock = 0;
  frame = undefined;
  vi.spyOn(performance, "now").mockImplementation(() => clock);
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((callback) => {
    frame = callback;
    return 1;
  });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {
    frame = undefined;
  });
  HTMLElement.prototype.setPointerCapture = (id) => {
    captured.add(id);
  };
  HTMLElement.prototype.hasPointerCapture = (id) => captured.has(id);
  HTMLElement.prototype.releasePointerCapture = (id) => {
    captured.delete(id);
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("selects all results with Ctrl+A in selection mode, but not from text fields or when disabled", () => {
  const interaction = vi.fn();
  const input = document.createElement("input");
  document.body.append(input);
  const view = renderSolid(Harness, { interaction });
  domEvent.keyDown(document.body, { key: "a", ctrlKey: true });
  expect(interaction).toHaveBeenLastCalledWith({ type: "select-all" });
  interaction.mockClear();
  domEvent.keyDown(document.body, { key: "A", metaKey: true });
  expect(interaction).toHaveBeenCalledWith({ type: "select-all" });
  interaction.mockClear();
  domEvent.keyDown(input, { key: "a", ctrlKey: true });
  domEvent.keyDown(document.body, { key: "a", ctrlKey: true, shiftKey: true });
  domEvent.keyDown(document.body, { key: "a" });
  expect(interaction).not.toHaveBeenCalledWith({ type: "select-all" });
  view.unmount();
  renderSolid(Harness, { interaction, enabled: false });
  domEvent.keyDown(document.body, { key: "a", ctrlKey: true });
  expect(interaction).not.toHaveBeenCalledWith({ type: "select-all" });
  input.remove();
});

describe("gallery pointer gestures", () => {
  it("maps a rectangle through the current logical shift without treating scroll as layout cancellation", () => {
    const interaction = vi.fn(), scrollBy = vi.fn();
    let shift = 11000;
    renderSolid(Harness, { interaction, shift: () => shift, scrollBy });
    layout();
    down();
    clock += 150;
    move(150, 150);
    expect(preview().ranges).toEqual([
      { startIndex: 300, endIndex: 301 }, { startIndex: 303, endIndex: 304 }
    ]);
    expect(preview().rectangle.top).toBe(15);
    shift = 11110;
    move(150, 150);
    tick();
    expect(preview().ranges).toEqual([
      { startIndex: 300, endIndex: 301 }, { startIndex: 303, endIndex: 304 },
      { startIndex: 306, endIndex: 307 }
    ]);
    move(150, 399);
    tick();
    expect(scrollBy).toHaveBeenCalledWith(expect.any(Number));
    expect(scrollBy.mock.lastCall?.[0]).toBeGreaterThan(0);
    up(150, 150);
    expect(interaction).toHaveBeenLastCalledWith(expect.objectContaining({type: "rectangle-commit"}));
  });
  it("requires both a 150 ms hold and five pixels of movement", () => {
    const interaction = vi.fn();
    renderSolid(Harness, { interaction: interaction });
    layout();
    down();
    clock += 149;
    move(18, 19);
    expect(preview()).toBeNull();
    expect(captured.size).toBe(0);
    expect(frame).toBeUndefined();
    clock += 1;
    expect(preview()).toBeNull();
    move(18, 17);
    expect(preview()).toBeNull();
    move(18, 19);
    expect(preview()).not.toBeNull();
    expect(captured.has(1)).toBe(true);
  });
  it("keeps quick repeated clicks immediate despite accidental movement", () => {
    const interaction = vi.fn();
    renderSolid(Harness, { interaction: interaction });
    layout();
    for (const detail of [1, 2]) {
      down();
      clock += 50;
      move(21, 15);
      up(21, 15);
      fireEvent.click(screen.getByText("tile"), { detail });
      expect(interaction).toHaveBeenLastCalledWith({
        type: "click",
        assetId: 1,
        assetIndex: 0,
        ctrlLike: false,
        shift: false
      });
      expect(preview()).toBeNull();
      expect(captured.size).toBe(0);
      expect(frame).toBeUndefined();
    }
    expect(interaction.mock.calls.filter(([event]) => event.type === "click")).toHaveLength(2);
    expect(interaction.mock.calls.some(([event]) => event.type === "rectangle-commit")).toBe(false);
    clock += 200;
    move(150, 150);
    expect(preview()).toBeNull();
    down();
    move(150, 150);
    expect(preview()).toBeNull();
    clock += 150;
    move(150, 150);
    expect(preview()).not.toBeNull();
  });
  it("keeps a stationary hold a click", () => {
    const interaction = vi.fn();
    renderSolid(Harness, { interaction: interaction });
    layout();
    down();
    clock += 1000;
    move(15, 15);
    expect(preview()).toBeNull();
    expect(frame).toBeUndefined();
    up(15, 15);
    fireEvent.click(screen.getByText("tile"), { detail: 1 });
    expect(interaction).toHaveBeenLastCalledWith({
      type: "click",
      assetId: 1,
      assetIndex: 0,
      ctrlLike: false,
      shift: false
    });
  });
  it.each([
    { ctrlKey: true },
    { metaKey: true },
    {}
  ])("captures modifiers, shrinks, commits once, and suppresses the generated click: %j", async (modifiers) => {
    let finish!: () => void;
    const interaction = vi.fn((event) =>
      event.type === "rectangle-commit"
        ? new Promise<void>((resolve) => {
            finish = resolve;
          })
        : undefined
    );
    renderSolid(Harness, { interaction: interaction });
    layout();
    down(15, 15, modifiers);
    clock += 150;
    move(240, 250);
    tick();
    expect(preview().ranges).toEqual([{ startIndex: 0, endIndex: 8 }]);
    move(125, 125);
    tick();
    expect(preview().ranges).toEqual([
      { startIndex: 0, endIndex: 1 },
      { startIndex: 3, endIndex: 4 }
    ]);
    up(125, 125);
    expect(interaction).toHaveBeenLastCalledWith({
      type: "rectangle-commit",
      additive: Object.keys(modifiers).length > 0,
      ranges: [
        { startIndex: 0, endIndex: 1 },
        { startIndex: 3, endIndex: 4 }
      ]
    });
    expect(preview()).not.toBeNull();
    interaction.mockClear();
    fireEvent.click(screen.getByText("tile"), { detail: 1, ...modifiers });
    expect(interaction).not.toHaveBeenCalled();
    await act(async () => finish());
    expect(preview()).toBeNull();
    fireEvent.click(screen.getByText("tile"), { detail: 0 });
    expect(interaction).toHaveBeenCalledOnce();
  });
  it.each([
    false,
    true
  ])("handles the first new click after a rectangle without a trailing click, bulk enabled: %s", async (enabled) => {
    const interaction = vi.fn();
    const onSelect = vi.fn();
    const view = renderSolid(Harness, { interaction: interaction, onSelect: onSelect });
    layout();
    down();
    clock += 150;
    move(125, 125);
    await act(async () => up(125, 125));
    expect(interaction).toHaveBeenLastCalledWith({
      type: "rectangle-commit",
      additive: false,
      ranges: [
        { startIndex: 0, endIndex: 1 },
        { startIndex: 3, endIndex: 4 }
      ]
    });
    // The browser never delivered the click belonging to the completed drag.
    view.rerender({ interaction: interaction, onSelect: onSelect, enabled: enabled });
    interaction.mockClear();
    down();
    up(15, 15);
    fireEvent.click(screen.getByText("tile"), { detail: 1 });
    if (enabled) {
      expect(interaction).toHaveBeenLastCalledWith({
        type: "click",
        assetId: 1,
        assetIndex: 0,
        ctrlLike: false,
        shift: false
      });
      expect(onSelect).not.toHaveBeenCalled();
    } else {
      expect(onSelect).toHaveBeenCalledExactlyOnceWith(asset);
      expect(interaction).not.toHaveBeenCalled();
    }
  });
  it.each([
    "pressed",
    "dragging",
    "pending"
  ])("Escape clears selection during %s", async (stage) => {
    const pending = stage === "pending";
    let finish: (() => void) | undefined;
    const interaction = vi.fn((event) =>
      event.type === "rectangle-commit"
        ? new Promise<void>((resolve) => {
            finish = resolve;
          })
        : undefined
    );
    renderSolid(Harness, { interaction: interaction });
    layout();
    down();
    if (stage !== "pressed") {
      clock += 150;
      move(150, 150);
    }
    if (pending) up(150, 150);
    if (stage !== "pressed") expect(preview()).not.toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(preview()).toBeNull();
    expect(captured.size).toBe(0);
    expect(frame).toBeUndefined();
    expect(interaction).toHaveBeenLastCalledWith({ type: "clear" });
    interaction.mockClear();
    if (pending) await act(async () => finish?.());
    else {
      up(150, 150);
      fireEvent.click(screen.getByText("tile"), { detail: 1 });
    }
    expect(preview()).toBeNull();
    expect(interaction).not.toHaveBeenCalled();
  });

  it("lets an open or locked modal own Escape before clearing the gallery", () => {
    const interaction = vi.fn();
    const dismiss = vi.fn();
    function ModalLayer(props: { locked: boolean }) {
      let node: HTMLDivElement | undefined;
      createUiLayer({
        active: () => true,
        modal: true,
        container: () => node,
        closeOnEscape: () => !props.locked,
        onEscape: dismiss
      });
      return (
        <div
          ref={(el) => {
            node = el;
          }}
        />
      );
    }
    function Layers(props: { modal: boolean; locked: boolean }) {
      return (
        <UiLayerProvider>
          <Harness interaction={interaction} />
          {props.modal ? <ModalLayer locked={props.locked} /> : null}
        </UiLayerProvider>
      );
    }
    const view = renderSolid(Layers, { modal: true, locked: false });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(dismiss).toHaveBeenCalledOnce();
    expect(interaction).not.toHaveBeenCalled();
    view.rerender({ modal: true, locked: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(dismiss).toHaveBeenCalledOnce();
    expect(interaction).not.toHaveBeenCalled();
    view.rerender({ modal: false, locked: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(interaction).toHaveBeenLastCalledWith({ type: "clear" });
  });

  it("selects through unloaded rows during edge scrolling", () => {
    renderSolid(Harness, { interaction: vi.fn() });
    const scroller = layout();
    down();
    clock += 150;
    move(330, 399);
    tick();
    expect(scroller.scrollTop).toBeGreaterThan(0);
    scroller.scrollTop = 66000;
    tick();
    expect(preview().ranges[0].endIndex).toBeGreaterThan(1536);
    expect(preview().rectangle.top).toBe(0);
    expect(preview().rectangle.height).toBeLessThanOrEqual(400);
  });
  describe.each(["pressed", "dragging"])("cancellation while %s", (stage) => {
    it.each([
      "pointerCancel",
      "lostPointerCapture",
      "blur",
      "query",
      "layout",
      "origin",
      "resize",
      "disabled",
      "unmount"
    ])("cancels on %s", (cause) => {
      const interaction = vi.fn();
      const view = renderSolid(Harness, { interaction: interaction });
      layout();
      down();
      clock += stage === "dragging" ? 150 : 149;
      move(150, 150);
      if (cause === "resize") fireEvent.resize(window);
      else if (cause === "blur") fireEvent.blur(window);
      else if (cause === "query") view.rerender({ interaction: interaction, epoch: 1 });
      else if (cause === "layout") view.rerender({ interaction: interaction, size: 110 });
      else if (cause === "origin") view.rerender({ interaction: interaction, offset: 20 });
      else if (cause === "disabled") view.rerender({ interaction: interaction, enabled: false });
      else if (cause === "unmount") view.unmount();
      else if (cause === "pointerCancel") fireEvent.pointerCancel(screen.getByTestId("gallery"));
      else fireEvent.lostPointerCapture(screen.getByTestId("gallery"));
      expect(interaction).toHaveBeenLastCalledWith({ type: "rectangle-cancel" });
      expect(captured.size).toBe(0);
      expect(frame).toBeUndefined();
      if (cause !== "unmount") {
        clock += 200;
        move(200, 200);
        up(200, 200);
        expect(preview()).toBeNull();
        expect(interaction.mock.calls.some(([event]) => event.type === "rectangle-commit")).toBe(
          false
        );
      }
    });
  });
  it("ignores controls, scrollbar, secondary pointers and normal-mode dragging", () => {
    const interaction = vi.fn();
    const onSelect = vi.fn();
    const view = renderSolid(Harness, { interaction: interaction, onSelect: onSelect });
    layout();
    fireEvent.pointerDown(screen.getByText("control"));
    fireEvent.click(screen.getByText("control"));
    down(340, 20);
    down(15, 15, { button: 2 });
    down(15, 15, { isPrimary: false });
    expect(interaction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("gallery"));
    expect(interaction).not.toHaveBeenCalled();
    view.rerender({ interaction: interaction, enabled: false, onSelect: onSelect });
    interaction.mockClear();
    down();
    move(200, 200);
    up(200, 200);
    fireEvent.click(screen.getByText("tile"));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(interaction).not.toHaveBeenCalled();
    expect(onSelect).toHaveBeenCalledOnce();
  });
});

const fireEvent = new Proxy(domEvent, {
  get(target, key: keyof typeof domEvent) {
    const call = target[key];
    return (...args: Parameters<typeof call>) => {
      const result = call(...args);
      flush();
      return result;
    };
  }
});

it.each([
  "query",
  "disabled",
  "press",
  "click",
  "unmount"
])("cancels a failed selection retry on explicit %s", async (cause) => {
  const load = vi.fn().mockRejectedValue(new Error("failed range"));
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => 0,
      identityEpoch: () => 0,
      sessionId: () => 1,
      getIdsRange: load
    })
  );
  selection.toggle();
  const interaction = selection.interact;
  const view = renderSolid(Harness, { interaction });
  layout();
  await selection.interact({
    type: "rectangle-commit",
    ranges: [{ startIndex: 0, endIndex: 1 }],
    additive: false
  });
  flush();
  expect(selection.error).toContain("failed range");
  if (cause === "query") view.rerender({ interaction, epoch: 1 });
  if (cause === "disabled") view.rerender({ interaction, enabled: false });
  if (cause === "press") down();
  if (cause === "click") fireEvent.click(screen.getByText("tile"));
  if (cause === "unmount") view.unmount();
  flush();
  expect(selection.error).toBeNull();
  await selection.retry();
  expect(load).toHaveBeenCalledOnce();
});

it("keeps a released rectangle read through layout changes while removing its old preview", async () => {
  let finish: (ids: number[]) => void = () => {};
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => 0,
      identityEpoch: () => 0,
      sessionId: () => 1,
      getIdsRange: () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    })
  );
  selection.toggle();
  const interaction = selection.interact;
  const view = renderSolid(Harness, { interaction });
  layout();
  down();
  clock += 150;
  move(125, 125);
  up(125, 125);
  expect(selection.busy).toBe(true);
  expect(preview()).not.toBeNull();
  view.rerender({ interaction, offset: 48 });
  expect(preview()).toBeNull();
  expect(selection.busy).toBe(true);
  await act(async () => finish([1, 2, 3, 4, 5]));
  expect([...selection.ids]).toEqual([1, 2, 4, 5]);
  expect(selection.busy).toBe(false);
});

it("keeps a released selection read through window blur", async () => {
  let finish: (ids: number[]) => void = () => {};
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => 0,
      identityEpoch: () => 0,
      sessionId: () => 1,
      getIdsRange: () =>
        new Promise((resolve) => {
          finish = resolve;
        })
    })
  );
  selection.toggle();
  renderSolid(Harness, { interaction: selection.interact });
  layout();
  down();
  clock += 150;
  move(125, 125);
  up(125, 125);
  expect(selection.busy).toBe(true);
  fireEvent.blur(window);
  expect(selection.busy).toBe(true);
  await act(async () => finish([1, 2, 3, 4, 5]));
  expect([...selection.ids]).toEqual([1, 2, 4, 5]);
  expect(selection.busy).toBe(false);
});

it("keeps a failed selection read and its Retry through window blur", async () => {
  const load = vi
    .fn()
    .mockRejectedValueOnce(new Error("failed range"))
    .mockResolvedValue([1, 2]);
  const { result: selection } = renderHook(() =>
    createSelection({
      queryEpoch: () => 0,
      identityEpoch: () => 0,
      sessionId: () => 1,
      getIdsRange: load
    })
  );
  selection.toggle();
  renderSolid(Harness, { interaction: selection.interact });
  layout();
  await selection.interact({
    type: "rectangle-commit",
    ranges: [{ startIndex: 0, endIndex: 1 }],
    additive: false
  });
  flush();
  expect(selection.error).toContain("failed range");
  fireEvent.blur(window);
  expect(selection.error).toContain("failed range");
  await selection.retry();
  expect(load).toHaveBeenCalledTimes(2);
  expect(selection.error).toBeNull();
});
