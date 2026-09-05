import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WindowControls } from "../WindowControls";

const native = vi.hoisted(() => ({
  available: true,
  resize: () => {},
  stop: vi.fn(),
  window: {
    isMaximized: vi.fn(), isFullscreen: vi.fn(), onResized: vi.fn(),
    minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(), startResizeDragging: vi.fn()
  }
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => native.available }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native.window }));

beforeEach(() => {
  native.available = true;
  native.window.isMaximized.mockResolvedValue(false);
  native.window.isFullscreen.mockResolvedValue(false);
  native.window.onResized.mockImplementation(async (callback: () => void) => {
    native.resize = callback;
    return native.stop;
  });
  for (const command of [native.window.minimize, native.window.toggleMaximize, native.window.close, native.window.startResizeDragging]) {
    command.mockResolvedValue(undefined);
  }
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("WindowControls", () => {
  it("runs native actions and follows externally changed maximized/fullscreen state", async () => {
    const view = render(<WindowControls />);
    fireEvent.click(screen.getByRole("button", { name: "Minimize window" }));
    fireEvent.click(screen.getByRole("button", { name: "Maximize window" }));
    expect(native.window.minimize).toHaveBeenCalledOnce();
    expect(native.window.toggleMaximize).toHaveBeenCalledOnce();
    const edge = document.querySelector('[data-resize-direction="SouthEast"]');
    if (!edge) throw new Error("Missing window resize edge");
    fireEvent.mouseDown(edge, { button: 2 });
    expect(native.window.startResizeDragging).not.toHaveBeenCalled();
    fireEvent.mouseDown(edge, { button: 0 });
    expect(native.window.startResizeDragging).toHaveBeenCalledWith("SouthEast");

    native.window.isMaximized.mockResolvedValue(true);
    act(() => native.resize());
    await screen.findByRole("button", { name: "Restore window" });
    expect(document.querySelector('[data-resize-direction]')).toBeNull();
    native.window.isMaximized.mockResolvedValue(false);
    native.window.isFullscreen.mockResolvedValue(true);
    act(() => native.resize());
    await screen.findByRole("button", { name: "Maximize window" });
    expect(document.querySelector('[data-resize-direction]')).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Close window" }));
    expect(native.window.close).toHaveBeenCalledOnce();
    view.unmount();
    expect(native.stop).toHaveBeenCalledOnce();
  });

  it("releases a listener even when registration finishes after unmount", async () => {
    let finish: ((stop: () => void) => void) | undefined;
    native.window.onResized.mockImplementation(() => new Promise<() => void>((resolve) => { finish = resolve; }));
    const view = render(<WindowControls />);
    view.unmount();
    finish?.(native.stop);
    await waitFor(() => expect(native.stop).toHaveBeenCalledOnce());
  });

  it("omits desktop actions in browser previews", () => {
    native.available = false;
    render(<WindowControls />);
    expect(screen.queryByRole("group", { name: "Window controls" })).not.toBeInTheDocument();
    expect(native.window.onResized).not.toHaveBeenCalled();
  });
});
