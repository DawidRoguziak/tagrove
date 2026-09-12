import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThumbnailImage, TRANSPARENT_THUMBNAIL_SRC } from "../ThumbnailImage";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function load(image: HTMLElement) {
  Object.defineProperty(image, "naturalWidth", { configurable: true, value: 100 });
  fireEvent.load(image);
}

describe("ThumbnailImage", () => {
  it("leaves fading disabled by default and forwards load events", () => {
    const onLoad = vi.fn();
    const { getByRole } = render(<ThumbnailImage src="a.jpg" alt="preview" className="object-cover" onLoad={onLoad} />);
    const image = getByRole("img");
    expect(image).toHaveClass("object-cover");
    expect(image).not.toHaveClass("thumbnail-fade-in");
    load(image);
    expect(onLoad).toHaveBeenCalledTimes(1);
    expect(image).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("waits for a delayed load before fading the image", () => {
    const onLoad = vi.fn();
    const { getByRole } = render(<ThumbnailImage src="a.jpg" alt="preview" fadeIn onLoad={onLoad} />);
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
    const { getByRole } = render(<ThumbnailImage src="cached.jpg" alt="preview" fadeIn />);
    expect(getByRole("img")).toHaveClass("thumbnail-fade-in-ready");
  });

  it("does not treat a completed failed request as decoded", () => {
    vi.spyOn(HTMLImageElement.prototype, "complete", "get").mockReturnValue(true);
    vi.spyOn(HTMLImageElement.prototype, "naturalWidth", "get").mockReturnValue(0);
    const { getByRole } = render(<ThumbnailImage src="broken.jpg" alt="preview" fadeIn />);
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("keeps the error fallback transparent and retries on source replacement", () => {
    const { getByRole, rerender } = render(<ThumbnailImage src="broken.jpg" alt="preview" fadeIn />);
    const failedImage = getByRole("img");
    fireEvent.error(failedImage);
    expect(failedImage).toHaveAttribute("src", TRANSPARENT_THUMBNAIL_SRC);
    load(failedImage);
    expect(failedImage).not.toHaveClass("thumbnail-fade-in-ready");
    rerender(<ThumbnailImage src="retry.jpg" alt="preview" fadeIn />);
    const retryImage = getByRole("img");
    expect(retryImage).toHaveAttribute("src", "retry.jpg");
    expect(retryImage).not.toHaveClass("thumbnail-fade-in-ready");
    load(retryImage);
    expect(retryImage).toHaveClass("thumbnail-fade-in-ready");
  });

  it("resets a loaded source but preserves the image and readiness on unchanged-source rerenders", () => {
    const { getByRole, rerender } = render(<ThumbnailImage src="a.jpg" alt="preview" fadeIn />);
    const firstImage = getByRole("img");
    load(firstImage);
    rerender(<ThumbnailImage src="a.jpg" alt="updated label" className="object-cover" fadeIn />);
    expect(getByRole("img")).toBe(firstImage);
    expect(firstImage).toHaveClass("thumbnail-fade-in-ready", "object-cover");
    rerender(<ThumbnailImage src="b.jpg" alt="preview" fadeIn />);
    expect(getByRole("img")).not.toBe(firstImage);
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
    load(getByRole("img"));
    expect(getByRole("img")).toHaveClass("thumbnail-fade-in-ready");
    rerender(<ThumbnailImage src="a.jpg" alt="preview" fadeIn />);
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
  });

  it("does not animate the transparent placeholder", () => {
    const { getByRole } = render(<ThumbnailImage src={TRANSPARENT_THUMBNAIL_SRC} alt="preview" fadeIn />);
    load(getByRole("img"));
    expect(getByRole("img")).not.toHaveClass("thumbnail-fade-in-ready");
  });
});
