import { renderSolid } from "../../../test/renderSolid";
import { createSignal, flush } from "solid-js";
import { cleanup, fireEvent, render } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ThumbnailImage,
  THUMBNAIL_MAX_RETRY_ATTEMPT,
  THUMBNAIL_RECOVERY_MAX_DELAY_MS,
  THUMBNAIL_RETRY_DELAYS_MS,
  thumbnailRetryDelay,
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

  it("reports readiness of the current source, never of the failure placeholder", () => {
    const onReady = vi.fn();
    const { getByRole, rerender } = renderSolid(ThumbnailImage, {
      src: "a.jpg",
      alt: "preview",
      onReady
    });
    const image = getByRole("img");
    expect(onReady).not.toHaveBeenCalled();
    load(image);
    expect(onReady).toHaveBeenCalledTimes(1);
    rerender({ src: "broken.jpg", alt: "preview", onReady });
    fireEvent.error(image);
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    load(image);
    expect(onReady).toHaveBeenCalledTimes(1);
    // A cached replacement is ready without a load event.
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
    rerender({ src: "cached.jpg", alt: "preview", onReady });
    expect(image).not.toHaveClass("invisible");
    expect(onReady).toHaveBeenCalledTimes(2);
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

  it("keeps retrying with a capped backoff after the quick retries until the picture loads", () => {
    vi.useFakeTimers();
    const onFailed = vi.fn();
    const onReady = vi.fn();
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "a",
      retrySrc: (attempt: number) => `a&r=${attempt}`,
      alt: "preview",
      onFailed,
      onReady
    });
    const image = getByRole("img");
    // A full render queue answers 503 for much longer than the quick retries (2.5 s).
    let elapsed = 0;
    for (let count = 0; count < 6; count += 1) {
      fireEvent.error(image);
      flush();
      if (count >= THUMBNAIL_RETRY_DELAYS_MS.length) {
        expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
        expect(image).not.toHaveClass("invisible");
      }
      const delay = thumbnailRetryDelay(count);
      vi.advanceTimersByTime(delay);
      elapsed += delay;
      flush();
      expect(image).toHaveAttribute("src", `a&r=${count + 1}`);
      expect(image).toHaveClass("invisible");
    }
    expect(elapsed).toBeGreaterThan(30_000);
    expect(onFailed).toHaveBeenCalledTimes(1);
    expect(onReady).not.toHaveBeenCalled();
    load(image);
    expect(image).toHaveAttribute("src", "a&r=6");
    expect(image).not.toHaveClass("invisible");
    expect(onReady).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("caps the retry delay and keeps the retry counter within three digits", () => {
    expect(THUMBNAIL_RETRY_DELAYS_MS.map((_, count) => thumbnailRetryDelay(count))).toEqual([
      ...THUMBNAIL_RETRY_DELAYS_MS
    ]);
    const delays = Array.from({ length: 40 }, (_, index) =>
      thumbnailRetryDelay(index + THUMBNAIL_RETRY_DELAYS_MS.length)
    );
    expect(delays.slice(0, 3)).toEqual([4000, 8000, 16_000]);
    expect(Math.max(...delays)).toBe(THUMBNAIL_RECOVERY_MAX_DELAY_MS);
    expect(delays.every((delay, index) => index === 0 || delay >= delays[index - 1])).toBe(true);

    vi.useFakeTimers();
    const attempts: number[] = [];
    const { getByRole } = renderSolid(ThumbnailImage, {
      src: "a",
      retrySrc: (attempt: number) => {
        attempts.push(attempt);
        return `a&r=${attempt}`;
      },
      alt: "preview"
    });
    const image = getByRole("img");
    for (let count = 0; count <= THUMBNAIL_MAX_RETRY_ATTEMPT; count += 1) {
      fireEvent.error(image);
      flush();
      vi.advanceTimersByTime(thumbnailRetryDelay(count));
      flush();
    }
    expect(image).toHaveAttribute("src", "a&r=1");
    expect(Math.max(...attempts)).toBe(THUMBNAIL_MAX_RETRY_ATTEMPT);
    expect(Math.min(...attempts)).toBe(1);
  });

  it("requests a source again when it comes back to a recycled element after a failure", () => {
    vi.useFakeTimers();
    const onFailed = vi.fn();
    const props = (src: string) => ({
      src,
      retrySrc: (attempt: number) => `${src}&r=${attempt}`,
      alt: "preview",
      onFailed
    });
    const { getByRole, rerender } = renderSolid(ThumbnailImage, props("a"));
    const image = getByRole("img");
    for (const delay of THUMBNAIL_RETRY_DELAYS_MS) {
      fireEvent.error(image);
      flush();
      vi.advanceTimersByTime(delay);
      flush();
    }
    fireEvent.error(image);
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    expect(onFailed).toHaveBeenCalledTimes(1);

    rerender(props("b"));
    expect(vi.getTimerCount()).toBe(0);
    expect(image).toHaveAttribute("src", "b");
    load(image);
    rerender(props("a"));
    expect(getByRole("img")).toBe(image);
    expect(image).toHaveAttribute("src", "a");
    expect(image).toHaveClass("invisible");
    load(image);
    expect(image).not.toHaveClass("invisible");
  });

  it("keeps a pending slower retry when an unchanged source is read again", () => {
    vi.useFakeTimers();
    const [version, setVersion] = createSignal({ id: 1 });
    const retrySrc = (attempt: number) => `a&r=${attempt}`;
    const onFailed = vi.fn();
    const { getByRole } = render(() => (
      <ThumbnailImage
        // A refreshed summary is a new object with the same URL.
        src={version().id === 1 ? "a" : "unused"}
        retrySrc={retrySrc}
        alt="preview"
        onFailed={onFailed}
      />
    ));
    const image = getByRole("img");
    for (const delay of THUMBNAIL_RETRY_DELAYS_MS) {
      fireEvent.error(image);
      flush();
      vi.advanceTimersByTime(delay);
      flush();
    }
    fireEvent.error(image);
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    setVersion({ id: 1 });
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(thumbnailRetryDelay(THUMBNAIL_RETRY_DELAYS_MS.length));
    flush();
    expect(image).toHaveAttribute("src", `a&r=${THUMBNAIL_RETRY_DELAYS_MS.length + 1}`);
    expect(onFailed).toHaveBeenCalledTimes(1);
  });

  it("requests a source again after a final failure without retries", () => {
    const { getByRole, rerender } = renderSolid(ThumbnailImage, { src: "a", alt: "preview" });
    const image = getByRole("img");
    fireEvent.error(image);
    flush();
    expect(image).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    rerender({ src: "b", alt: "preview" });
    rerender({ src: "a", alt: "preview" });
    expect(image).toHaveAttribute("src", "a");
  });

  it("cancels a pending retry on unmount", () => {
    vi.useFakeTimers();
    const retrySrc = vi.fn((attempt: number) => `a&r=${attempt}`);
    const { getByRole, unmount } = renderSolid(ThumbnailImage, {
      src: "a",
      retrySrc,
      alt: "preview"
    });
    const image = getByRole("img");
    for (const delay of THUMBNAIL_RETRY_DELAYS_MS) {
      fireEvent.error(image);
      flush();
      vi.advanceTimersByTime(delay);
      flush();
    }
    fireEvent.error(image);
    flush();
    expect(vi.getTimerCount()).toBe(1);
    const calls = retrySrc.mock.calls.length;
    unmount();
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(THUMBNAIL_RECOVERY_MAX_DELAY_MS * 4);
    flush();
    expect(retrySrc).toHaveBeenCalledTimes(calls);
  });
});
