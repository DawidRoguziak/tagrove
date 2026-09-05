import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useLightboxSidebar } from "../useLightboxSidebar";

describe("lightbox sidebar", () => {
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("follows breakpoints until a user chooses, then retains that choice until close", () => {
    let change = () => {};
    const query = {
      matches: false,
      addEventListener: (_: string, cb: () => void) => {
        change = cb;
      },
      removeEventListener: vi.fn()
    };
    vi.stubGlobal("matchMedia", (name: string) =>
      name.includes("max-width") ? query : { matches: false }
    );
    const { result, rerender } = renderHook(
      ({ active, locked }) => useLightboxSidebar(active, locked),
      {
        initialProps: { active: true, locked: false }
      }
    );
    expect(result.current.sidebarOpen).toBe(true);
    act(() => {
      query.matches = true;
      change();
    });
    expect(result.current.sidebarOpen).toBe(false);
    act(() => result.current.openSidebar());
    act(() => {
      query.matches = false;
      change();
    });
    expect(result.current.sidebarOpen).toBe(true);
    act(() => result.current.closeSidebar());
    rerender({ active: true, locked: false });
    expect(result.current.sidebarOpen).toBe(false);
    rerender({ active: true, locked: true });
    act(() => result.current.closeSidebar());
    expect(result.current.sidebarOpen).toBe(true);
    rerender({ active: false, locked: false });
    rerender({ active: true, locked: false });
    expect(result.current.sidebarOpen).toBe(true);
  });

  it("keeps native video out of the panel until the closing animation ends", () => {
    vi.useFakeTimers();
    const { result } = renderHook(() => useLightboxSidebar(true, false));
    act(() => result.current.closeSidebar());
    expect(result.current.sidebarOpen).toBe(false);
    expect(result.current.sidebarOccupied).toBe(true);
    act(() => vi.advanceTimersByTime(199));
    expect(result.current.sidebarOccupied).toBe(true);
    act(() => result.current.openSidebar());
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.sidebarOccupied).toBe(true);
    act(() => result.current.closeSidebar());
    act(() => vi.advanceTimersByTime(200));
    expect(result.current.sidebarOccupied).toBe(false);
  });
});
