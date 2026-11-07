import { renderSolid } from "../../../test/renderSolid";
import { flush } from "solid-js";
import { cleanup, fireEvent } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ThumbnailImage,
  THUMBNAIL_RETRY_DELAYS_MS,
  TRANSPARENT_THUMBNAIL_SRC
} from "../ThumbnailImage";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function load(image: HTMLElement) {
  Object.defineProperty(image, "naturalWidth", { configurable: true, value: 100 });
  fireEvent.load(image);
  flush();
}

describe("ThumbnailImage", () => {
  it("leaves fading disabled by default and forwards load events", () => {
    const onLoad = vi.fn();
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "a.jpg",
      alt: "preview",
      className: "object-cover",
      onLoad: onLoad
    });
    const image = getByRole("img");
    expect(image).toHaveClass("object-cover");
    expect(image).not.toHaveClass("thumbnail-fade-in");
    load(image);
    expect(onLoad).toHaveBeenCalledTimes(1);
    expect(image).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("waits for a delayed load before fading the image", () => {
    const onLoad = vi.fn();
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "a.jpg",
      alt: "preview",
      fadeIn: true,
      onLoad: onLoad
    });
    const image = getByRole("img");
    expect(image).toHaveClass("thumbnail-fade-in");
    expect(image).not.toHaveClass("thumbnail-fade-in-ready");
    load(image);
    expect(image).toHaveClass("thumbnail-fade-in-ready");
    expect(onLoad).toHaveBeenCalledTimes(1);
  });

  it("recognizes an already decoded cached image without a load event", () => {
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
    vi.spyOn(HTMLImageElement.prototype, "naturalWidth", "get").mockReturnValue(100);
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "cached.jpg",
      alt: "preview",
      fadeIn: true
    });
    expect(getByRole("img")).toHaveClass("thumbnail-fade-in-ready");
  });

  it("does not treat a completed failed request as decoded", () => {
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
    vi.spyOn(HTMLImageElement.prototype, "naturalWidth", "get").mockReturnValue(0);
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "broken.jpg",
      alt: "preview",
      fadeIn: true
    });
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("keeps the error fallback transparent and retries on source replacement", () => {
    const { getByRole, rerender } = renderSolid(ThumbnailImage, {
      src: "broken.jpg",
      alt: "preview",
      fadeIn: true
    });
    const failedImage = getByRole("img");
    fireEvent.error(failedImage);
    flush();
    expect(failedImage).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    load(failedImage);
    expect(failedImage).not.toHaveClass("thumbnail-fade-in-ready");
    rerender({ src: "retry.jpg", alt: "preview", fadeIn: true });
    const retryImage = getByRole("img");
    expect(retryImage).toHaveAttribute("src", "retry.jpg");
    expect(retryImage).not.toHaveClass("thumbnail-fade-in-ready");
    load(retryImage);
    expect(retryImage).toHaveClass("thumbnail-fade-in-ready");
  });

  it("resets readiness per source while keeping one image element", () => {
    const { getByRole, rerender } = renderSolid(ThumbnailImage, {
      src: "a.jpg",
      alt: "preview",
      fadeIn: true
    });
    const image = getByRole("img");
    load(image);
    rerender({ src: "a.jpg", alt: "updated label", className: "object-cover", fadeIn: true });
    expect(getByRole("img")).toBe(image);
    expect(image).toHaveClass("thumbnail-fade-in-ready", "object-cover");
    rerender({ src: "b.jpg", alt: "preview", fadeIn: true });
    // Recycled gallery cells replace the attribute instead of creating an element.
    expect(getByRole("img")).toBe(image);
    expect(image).toHaveAttribute("src", "b.jpg");
    expect(image).not.toHaveClass("thumbnail-fade-in-ready");
    load(image);
    expect(image).toHaveClass("thumbnail-fade-in-ready");
    rerender({ src: "a.jpg", alt: "preview", fadeIn: true });
    expect(getByRole("img")).toBe(image);
    expect(image).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("decodes asynchronously and hides the previous picture until the new source loads", () => {
    const { getByRole, rerender } = renderSolid(ThumbnailImage, { src: "a.jpg", alt: "preview" });
    const image = getByRole("img");
    expect(image).toHaveAttribute("decoding", "async");
    expect(image).toHaveAttribute("crossorigin", "anonymous");
    expect(image).toHaveClass("invisible");
    load(image);
    expect(image).not.toHaveClass("invisible");
    rerender({ src: "b.jpg", alt: "preview" });
    expect(image).toHaveClass("invisible");
    // A late load event for an earlier source does not reveal the new one.
    image.setAttribute("src", "a.jpg");
    load(image);
    image.setAttribute("src", "b.jpg");
    expect(image).toHaveClass("invisible");
    load(image);
    expect(image).not.toHaveClass("invisible");
    rerender({ src: TRANSPARENT_THUMBNAIL_SRC, alt: "preview" });
    expect(image).not.toHaveClass("invisible");
  });

  it("reveals a cached replacement without waiting for a load event", () => {
    const { getByRole, rerender } = renderSolid(ThumbnailImage, { src: "a.jpg", alt: "preview" });
    const image = getByRole("img");
    load(image);
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
    rerender({ src: "cached.jpg", alt: "preview" });
    expect(image).toHaveAttribute("src", "cached.jpg");
    expect(image).not.toHaveClass("invisible");
  });

  it("does not animate the transparent placeholder", () => {
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: TRANSPARENT_THUMBNAIL_SRC,
      alt: "preview",
      fadeIn: true
    });
    load(getByRole("img"));
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("retries a failed load with backoff on the same element, then reports failure", () => {
    vi.useFakeTimers();
    const onFailed = vi.fn();
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "thumb://localhost/1?v=a",
      retrySrc: (attempt: number) => `thumb://localhost/1?v=a&r=${attempt}`,
      alt: "preview",
      onFailed
    });
    const image = getByRole("img");
    for (const [index, delay] of THUMBNAIL_RETRY_DELAYS_MS.entries()) {
      fireEvent.error(image);
      flush();
      vi.advanceTimersByTime(delay - 1);
      flush();
      expect(image).toHaveAttribute(
        "src",
        index === 0 ? "thumb://localhost/1?v=a" : `thumb://localhost/1?v=a&r=${index}`
      );
      vi.advanceTimersByTime(1);
      flush();
      expect(getByRole("img")).toBe(image);
      expect(image).toHaveAttribute("src", `thumb://localhost/1?v=a&r=${index + 1}`);
    }
    expect(onFailed).not.toHaveBeenCalled();
    fireEvent.error(image);
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    expect(onFailed).toHaveBeenCalledTimes(1);
  });

  it("reveals a retried load and drops a pending retry when the source changes", () => {
    vi.useFakeTimers();
    const { getByRole, rerender } = renderSolid(ThumbnailImage, {
      src: "a",
      retrySrc: (attempt: number) => `a&r=${attempt}`,
      alt: "preview"
    });
    const image = getByRole("img");
    fireEvent.error(image);
    vi.advanceTimersByTime(THUMBNAIL_RETRY_DELAYS_MS[0]);
    flush();
    expect(image).toHaveAttribute("src", "a&r=1");
    load(image);
    expect(image).not.toHaveClass("invisible");

    fireEvent.error(image);
    rerender({ src: "b", retrySrc: (attempt: number) => `b&r=${attempt}`, alt: "preview" });
    vi.advanceTimersByTime(THUMBNAIL_RETRY_DELAYS_MS[1]);
    flush();
    expect(image).toHaveAttribute("src", "b");
  });
});
