import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import type { MediaPlayerInstance } from "@vidstack/react";
import type { MutableRefObject } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelectedAsset } from "../../../types";
import { LightboxMediaStage } from "../LightboxMediaStage";

const apiMocks = vi.hoisted(() => ({
  getVideoStreamUrl: vi.fn()
}));

const videoPlayerMocks = vi.hoisted(() => ({
  activations: [] as Array<{ src: string; onError: () => void }>
}));

vi.mock("../../../api", () => ({
  getVideoStreamUrl: apiMocks.getVideoStreamUrl,
  toMediaSrc: (path: string) => `media://${path}`
}));

vi.mock("../LightboxVideoPlayer", () => ({
  LightboxVideoPlayer: ({ src, onError }: { src: string; onError: () => void }) => {
    videoPlayerMocks.activations.push({ src, onError });
    return <div data-testid="mock-video-player" data-src={src} />;
  }
}));

function createVideo(id: number): SelectedAsset {
  return {
    id,
    file_name: `${id}.mp4`,
    preview_path: null,
    kind: "video",
    modified_at: id,
    width: 160,
    height: 90,
    duration_ms: 10_000,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    path: `C:/media/${id}.mp4`,
    size_bytes: 100,
    tags: []
  };
}

function stageProps(selected: SelectedAsset) {
  return {
    selected,
    mediaViewportRef: { current: null } as MutableRefObject<HTMLDivElement | null>,
    lightboxImageRef: { current: null } as MutableRefObject<HTMLImageElement | null>,
    lightboxVideoPlayerRef: { current: null } as MutableRefObject<MediaPlayerInstance | null>,
    isZoomed: false,
    mediaDisplaySize: { width: 160, height: 90 },
    isDragging: false,
    isFullscreen: false,
    onImageLoad: vi.fn(),
    onVideoLoadedMetadata: vi.fn(),
    onVideoFullscreenChange: vi.fn(),
    onImageClick: vi.fn(),
    onImagePointerDown: vi.fn(),
    onImagePointerMove: vi.fn(),
    onImagePointerEnd: vi.fn()
  };
}

describe("LightboxMediaStage", () => {
  beforeEach(() => {
    apiMocks.getVideoStreamUrl.mockReset().mockImplementation(
      async (assetId: number) => `http://video/${assetId}.mp4`
    );
    videoPlayerMocks.activations.length = 0;
  });

  afterEach(cleanup);

  it("retries A after A to B to A and ignores a late error from the first activation", async () => {
    const first = createVideo(1);
    const second = createVideo(2);
    const { rerender } = render(<LightboxMediaStage {...stageProps(first)} />);

    await waitFor(() => expect(screen.getByTestId("mock-video-player")).toHaveAttribute(
      "data-src",
      "http://video/1.mp4"
    ));
    const staleFirstError = videoPlayerMocks.activations.at(-1)!.onError;
    act(() => staleFirstError());
    expect(screen.getByTestId("lightbox-media-error")).toBeInTheDocument();

    rerender(<LightboxMediaStage {...stageProps(second)} />);
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("mock-video-player")).toHaveAttribute(
      "data-src",
      "http://video/2.mp4"
    ));

    rerender(<LightboxMediaStage {...stageProps(first)} />);
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("mock-video-player")).toHaveAttribute(
      "data-src",
      "http://video/1.mp4"
    ));
    const currentFirstError = videoPlayerMocks.activations.at(-1)!.onError;

    act(() => staleFirstError());
    expect(screen.queryByTestId("lightbox-media-error")).not.toBeInTheDocument();
    act(() => currentFirstError());
    expect(screen.getByTestId("lightbox-media-error")).toBeInTheDocument();
  });
});
