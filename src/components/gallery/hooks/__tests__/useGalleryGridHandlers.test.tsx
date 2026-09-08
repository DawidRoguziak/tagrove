import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UiLayerProvider, useUiLayer } from "../../../UI/UiLayerProvider";
import type { BulkSelectionHandler } from "../../selection";
import { useGalleryGridHandlers } from "../useGalleryGridHandlers";

class TestPointerEvent extends MouseEvent {
  pointerId: number;
  isPrimary: boolean;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.isPrimary = init.isPrimary ?? true;
  }
}
const asset = { id: 1, file_name: "one.png", preview_path: null, kind: "image" as const, modified_at: 0,
  width: 100, height: 100, duration_ms: null, thumb_path: null, is_favorite: false, media_group_key: null, media_group_order: null };
let frame: FrameRequestCallback | undefined;
let clock = 0;
const captured = new Set<number>();
function Harness({ interaction, epoch = 0, offset = 0, size = 100, enabled = true, onSelect = vi.fn() }: {
  interaction: BulkSelectionHandler; epoch?: number; offset?: number; size?: number; enabled?: boolean; onSelect?: () => void;
}) {
  const gridRef = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const handlers = useGalleryGridHandlers({ assets: [asset], assetCount: 3000, queryEpoch: epoch, gridOffset: offset,
    gridRef, scrollContainerRef: scroller, columnCount: 3, tilePixelSize: size, tileGap: 10,
    selectionModeEnabled: enabled, onSelect, onBulkSelectionInteraction: interaction });
  return <div ref={scroller} data-testid="scroller"><section data-testid="gallery"
    onPointerDown={handlers.handlePointerDown} onPointerMove={handlers.handlePointerMove}
    onPointerUp={handlers.handlePointerUp} onPointerCancel={handlers.handlePointerCancel}
    onLostPointerCapture={handlers.handleLostPointerCapture} onClickCapture={handlers.handleClickCapture}
    onClick={handlers.handleGalleryClick}>
    <div ref={gridRef} data-testid="grid"><button data-asset-id="1" data-asset-index="0" onClick={handlers.handleTileClick}>tile</button></div>
    <button>control</button><output>{JSON.stringify(handlers.preview)}</output>
  </section></div>;
}
function bounds(left: number, top: number, width: number, height: number): DOMRect {
  return new DOMRect(left, top, width, height);
}
function layout() {
  const scroller = screen.getByTestId("scroller");
  Object.defineProperties(scroller, { clientWidth: { value: 340 }, clientHeight: { value: 400 } });
  vi.spyOn(scroller, "getBoundingClientRect").mockImplementation(() => bounds(0, 0, 340, 400));
  vi.spyOn(screen.getByTestId("grid"), "getBoundingClientRect").mockImplementation(() => bounds(10, 10 - scroller.scrollTop, 320, 110000));
  vi.spyOn(screen.getByTestId("gallery"), "getBoundingClientRect").mockImplementation(() => bounds(0, 0, 340, 110000));
  return scroller;
}
function down(x = 15, y = 15, modifiers = {}) { fireEvent.pointerDown(screen.getByText("tile"), { clientX: x, clientY: y, ...modifiers }); }
function move(x: number, y: number) { fireEvent.pointerMove(screen.getByTestId("gallery"), { clientX: x, clientY: y }); }
function up(x: number, y: number) { fireEvent.pointerUp(screen.getByTestId("gallery"), { clientX: x, clientY: y }); }
function tick() { act(() => { clock += 16; frame?.(clock); }); }
function preview() { return JSON.parse(screen.getByRole("status").textContent ?? "null"); }

beforeEach(() => {
  captured.clear(); clock = performance.now(); frame = undefined;
  vi.stubGlobal("PointerEvent", TestPointerEvent);
  vi.spyOn(window, "requestAnimationFrame").mockImplementation(callback => { frame = callback; return 1; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => { frame = undefined; });
  HTMLElement.prototype.setPointerCapture = id => { captured.add(id); };
  HTMLElement.prototype.hasPointerCapture = id => captured.has(id);
  HTMLElement.prototype.releasePointerCapture = id => { captured.delete(id); };
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("gallery pointer gestures", () => {
  it("keeps sub-threshold movement a click and starts at five pixels", () => {
    const interaction = vi.fn(); render(<Harness interaction={interaction} />); layout();
    down(); move(18, 17); expect(preview()).toBeNull(); up(18, 17);
    fireEvent.click(screen.getByText("tile"));
    expect(interaction).toHaveBeenLastCalledWith({ type: "click", assetId: 1, assetIndex: 0, ctrlLike: false, shift: false });
    down(); move(18, 19); expect(preview()).not.toBeNull(); expect(captured.has(1)).toBe(true);
  });
  it.each([{ ctrlKey: true }, { metaKey: true }, {}])("captures modifiers, shrinks, commits once, and suppresses the generated click: %j", async modifiers => {
    let finish!: () => void;
    const interaction = vi.fn(event => event.type === "rectangle-commit" ? new Promise<void>(resolve => { finish = resolve; }) : undefined);
    render(<Harness interaction={interaction} />); layout();
    down(15, 15, modifiers); move(240, 250); tick();
    expect(preview().ranges).toEqual([{ startIndex: 0, endIndex: 8 }]);
    move(125, 125); tick();
    expect(preview().ranges).toEqual([{ startIndex: 0, endIndex: 1 }, { startIndex: 3, endIndex: 4 }]);
    up(125, 125);
    expect(interaction).toHaveBeenLastCalledWith({ type: "rectangle-commit", additive: Object.keys(modifiers).length > 0,
      ranges: [{ startIndex: 0, endIndex: 1 }, { startIndex: 3, endIndex: 4 }] });
    expect(preview()).not.toBeNull();
    interaction.mockClear();
    fireEvent.click(screen.getByText("tile"), { detail: 1, ...modifiers });
    expect(interaction).not.toHaveBeenCalled();
    await act(async () => finish());
    expect(preview()).toBeNull();
    fireEvent.click(screen.getByText("tile"), { detail: 0 });
    expect(interaction).toHaveBeenCalledOnce();
  });
  it.each(["pressed", "dragging", "pending"])("Escape clears selection during %s", async stage => {
    const pending = stage === "pending";
    let finish: (() => void) | undefined;
    const interaction = vi.fn(event => event.type === "rectangle-commit"
      ? new Promise<void>(resolve => { finish = resolve; }) : undefined);
    render(<Harness interaction={interaction} />); layout();
    down();
    if (stage !== "pressed") move(150, 150);
    if (pending) up(150, 150);
    if (stage !== "pressed") expect(preview()).not.toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(preview()).toBeNull();
    expect(captured.size).toBe(0); expect(frame).toBeUndefined();
    expect(interaction).toHaveBeenLastCalledWith({ type: "clear" });
    interaction.mockClear();
    if (pending) await act(async () => finish?.());
    else {
      up(150, 150);
      fireEvent.click(screen.getByText("tile"), { detail: 1 });
    }
    expect(preview()).toBeNull(); expect(interaction).not.toHaveBeenCalled();
  });

  it("lets an open or locked modal own Escape before clearing the gallery", () => {
    const interaction = vi.fn(); const dismiss = vi.fn();
    function ModalLayer({ locked }: { locked: boolean }) {
      const ref = useRef<HTMLDivElement>(null);
      useUiLayer({ active: true, modal: true, containerRef: ref, closeOnEscape: !locked, onEscape: dismiss });
      return <div ref={ref} />;
    }
    const view = render(<UiLayerProvider><Harness interaction={interaction} /><ModalLayer locked={false} /></UiLayerProvider>);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(dismiss).toHaveBeenCalledOnce(); expect(interaction).not.toHaveBeenCalled();
    view.rerender(<UiLayerProvider><Harness interaction={interaction} /><ModalLayer locked /></UiLayerProvider>);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(dismiss).toHaveBeenCalledOnce(); expect(interaction).not.toHaveBeenCalled();
    view.rerender(<UiLayerProvider><Harness interaction={interaction} /></UiLayerProvider>);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(interaction).toHaveBeenLastCalledWith({ type: "clear" });
  });

  it("selects through unloaded rows during edge scrolling", () => {
    render(<Harness interaction={vi.fn()} />); const scroller = layout();
    down(); move(330, 399); tick(); expect(scroller.scrollTop).toBeGreaterThan(0);
    scroller.scrollTop = 66000; tick();
    expect(preview().ranges[0].endIndex).toBeGreaterThan(1536);
    expect(preview().rectangle.top).toBe(0);
    expect(preview().rectangle.height).toBeLessThanOrEqual(400);
  });
  it.each(["pointerCancel", "lostPointerCapture", "blur", "query", "layout", "origin", "unmount"])("cancels on %s", cause => {
    const interaction = vi.fn(); const view = render(<Harness interaction={interaction} />); layout(); down(); move(150, 150);
    if (cause === "blur") fireEvent.blur(window);
    else if (cause === "query") view.rerender(<Harness interaction={interaction} epoch={1} />);
    else if (cause === "layout") view.rerender(<Harness interaction={interaction} size={110} />);
    else if (cause === "origin") view.rerender(<Harness interaction={interaction} offset={20} />);
    else if (cause === "unmount") view.unmount();
    else if (cause === "pointerCancel") fireEvent.pointerCancel(screen.getByTestId("gallery"));
    else fireEvent.lostPointerCapture(screen.getByTestId("gallery"));
    expect(interaction).toHaveBeenLastCalledWith({ type: "rectangle-cancel" });
    expect(captured.size).toBe(0); expect(frame).toBeUndefined();
    if (cause !== "unmount") expect(preview()).toBeNull();
  });
  it("ignores controls, scrollbar, secondary pointers and normal-mode dragging", () => {
    const interaction = vi.fn(); const onSelect = vi.fn();
    const view = render(<Harness interaction={interaction} onSelect={onSelect} />); layout();
    fireEvent.pointerDown(screen.getByText("control")); fireEvent.click(screen.getByText("control"));
    down(340, 20); down(15, 15, { button: 2 }); down(15, 15, { isPrimary: false });
    expect(interaction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("gallery")); expect(interaction).toHaveBeenLastCalledWith({ type: "clear" });
    view.rerender(<Harness interaction={interaction} enabled={false} onSelect={onSelect} />);
    interaction.mockClear(); down(); move(200, 200); up(200, 200); fireEvent.click(screen.getByText("tile"));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(interaction).not.toHaveBeenCalled(); expect(onSelect).toHaveBeenCalledOnce();
  });
});
