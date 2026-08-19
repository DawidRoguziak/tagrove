import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";
import i18n from "../i18n";

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

void i18n.changeLanguage("en");
