import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

const storage = new Map<string, string>();
const localStorage: Storage = {
  get length() {
    return storage.size;
  },
  clear: () => storage.clear(),
  getItem: (key) => storage.get(key) ?? null,
  key: (index) => Array.from(storage.keys())[index] ?? null,
  removeItem: (key) => {
    storage.delete(key);
  },
  setItem: (key, value) => {
    storage.set(key, value);
  }
};
Object.defineProperty(window, "localStorage", { configurable: true, value: localStorage });

class MockIntersectionObserver implements IntersectionObserver {
  readonly root: Element | null = null;
  readonly rootMargin = "0px";
  readonly thresholds = [0];

  disconnect(): void {}
  observe(): void {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
  unobserve(): void {}
}

class MockResizeObserver implements ResizeObserver {
  disconnect(): void {}
  observe(): void {}
  unobserve(): void {}
}

vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
vi.stubGlobal("ResizeObserver", MockResizeObserver);

if (!window.requestAnimationFrame) {
  window.requestAnimationFrame = (callback: FrameRequestCallback) => {
    return window.setTimeout(() => callback(performance.now()), 16);
  };
}

if (!window.cancelAnimationFrame) {
  window.cancelAnimationFrame = (id: number) => {
    window.clearTimeout(id);
  };
}

const { default: i18n } = await import("../i18n");
void i18n.changeLanguage("en");
