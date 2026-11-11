import { createEffect, createMemo, createSignal, omit, onCleanup } from "solid-js";
import type { JSX } from "@solidjs/web";
export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
/**
 * Delays before the quick retries of a failed load. After them the transparent
 * placeholder is shown and `onFailed` is called.
 */
export const THUMBNAIL_RETRY_DELAYS_MS = [500, 2000] as const;
/** First delay of the slower retries that follow the quick ones; it doubles each time. */
export const THUMBNAIL_RECOVERY_DELAY_MS = 4000;
/** Cap of the slower retries' delay. */
export const THUMBNAIL_RECOVERY_MAX_DELAY_MS = 30_000;
/** Largest `retrySrc` attempt; later attempts cycle back to 1 (`r` has at most 3 digits). */
export const THUMBNAIL_MAX_RETRY_ATTEMPT = 999;
/** Delay before the retry that follows failed attempt `count` (0 is the first load). */
export function thumbnailRetryDelay(count: number): number {
  const quick = THUMBNAIL_RETRY_DELAYS_MS[count];
  if (quick !== undefined) return quick;
  const doublings = Math.min(count - THUMBNAIL_RETRY_DELAYS_MS.length, 16);
  return Math.min(THUMBNAIL_RECOVERY_DELAY_MS * 2 ** doublings, THUMBNAIL_RECOVERY_MAX_DELAY_MS);
}
type Props = Omit<JSX.ImgHTMLAttributes<HTMLImageElement>, "src" | "onError" | "onLoad"> & {
  src: string;
  /**
   * URL for retry `attempt` (1 to `THUMBNAIL_MAX_RETRY_ATTEMPT`) after a failed load;
   * without it a failure is final.
   */
  retrySrc?: (attempt: number) => string;
  fadeIn?: boolean;
  className?: string;
  onLoad?: (event: Event & { currentTarget: HTMLImageElement }) => void;
  /** Called when the picture of the current `src` (or one of its retries) is shown. */
  onReady?: () => void;
  /** Called once each time a `src` is shown, after its load and the quick retries failed. */
  onFailed?: () => void;
};
/**
 * One `<img>` for the component's lifetime. A new `src` replaces the attribute; the
 * previous picture stays hidden until the new one has loaded, so recycled gallery
 * cells never show another asset's thumbnail. A failed load is retried with backoff
 * through `retrySrc` (a 503 or a render that lost a race). After the quick retries
 * it shows a transparent placeholder, and keeps retrying with a capped backoff while
 * the component is mounted and `src` is unchanged: an `<img>` cannot tell a
 * transient 503 (full render queue) from a final 404. A new `src` drops that state,
 * so a source that returns later is requested again.
 */
export function ThumbnailImage(props: Props) {
  const rest = omit(
    props,
    "src",
    "retrySrc",
    "fadeIn",
    "className",
    "onLoad",
    "onReady",
    "onFailed"
  );
  let image: HTMLImageElement | undefined;
  let retryTimer: ReturnType<typeof setTimeout> | undefined;
  // Load, retry and failure state belong to the source they were observed for.
  const [loaded, setLoaded] = createSignal<string | null>(null);
  /**
   * A new object each time the `src` string changes (not on every re-read of an
   * unchanged URL, such as a refreshed summary). Retry and failure state are keyed by it,
   * so a source that comes back to this (recycled) element is requested again
   * instead of being answered by the placeholder from memory.
   */
  const source = createMemo<{ src: string }>((previous) =>
    previous?.src === props.src ? previous : { src: props.src }
  );
  type Attempt = { source: { src: string }; count: number };
  /** The attempt whose error put the placeholder up (the next attempt hides it). */
  const [failed, setFailed] = createSignal<Attempt | null>(null);
  const [attempt, setAttempt] = createSignal<Attempt | null>(null);
  const attemptCount = () => {
    const current = attempt();
    return current && current.source === source() ? current.count : 0;
  };
  /** The URL actually requested for the current `src`. */
  const requested = () => {
    const count = attemptCount();
    return count > 0 && props.retrySrc
      ? props.retrySrc(((count - 1) % THUMBNAIL_MAX_RETRY_ATTEMPT) + 1)
      : props.src;
  };
  const isFailed = () => {
    const current = failed();
    return current !== null && current.source === source() && current.count === attemptCount();
  };
  const shown = () => (isFailed() ? TRANSPARENT_THUMBNAIL_SRC : requested());
  const ready = () =>
    loaded() === props.src && !isFailed() && props.src !== TRANSPARENT_THUMBNAIL_SRC;
  const hidden = () => !ready() && shown() !== TRANSPARENT_THUMBNAIL_SRC;
  const clearRetry = () => {
    if (retryTimer !== undefined) clearTimeout(retryTimer);
    retryTimer = undefined;
  };
  const markLoaded = () => {
    setLoaded(props.src);
    if (props.src !== TRANSPARENT_THUMBNAIL_SRC) props.onReady?.();
  };
  onCleanup(clearRetry);
  // A new source drops the previous one's pending retry.
  createEffect(source, clearRetry, { defer: true });
  // A cached picture can be complete without a load event once the attribute changes.
  createEffect(
    () => shown(),
    (src) => {
      if (
        src !== TRANSPARENT_THUMBNAIL_SRC &&
        image?.complete &&
        image.naturalWidth > 0 &&
        image.getAttribute("src") === src
      )
        markLoaded();
    }
  );
  return (
    <img
      decoding="async"
      {...rest}
      crossorigin="anonymous"
      ref={(element) => {
        image = element;
      }}
      class={`${props.className ?? ""}${props.fadeIn ? ` thumbnail-fade-in${ready() ? " thumbnail-fade-in-ready" : ""}` : ""}${hidden() ? " invisible" : ""}`}
      src={shown()}
      onLoad={(event) => {
        if (
          event.currentTarget.naturalWidth > 0 &&
          event.currentTarget.getAttribute("src") === requested()
        )
          markLoaded();
        props.onLoad?.(event);
      }}
      onError={(event) => {
        if (event.currentTarget.getAttribute("src") !== requested()) return;
        const current = source();
        const count = attemptCount();
        if (!props.retrySrc) {
          setFailed({ source: current, count });
          props.onFailed?.();
          return;
        }
        // During the quick retries the failed URL stays (hidden, with the tile's
        // spinner); afterwards the placeholder shows between the slower retries.
        if (count >= THUMBNAIL_RETRY_DELAYS_MS.length) setFailed({ source: current, count });
        if (count === THUMBNAIL_RETRY_DELAYS_MS.length) props.onFailed?.();
        clearRetry();
        retryTimer = setTimeout(() => {
          retryTimer = undefined;
          if (source() === current && attemptCount() === count)
            setAttempt({ source: current, count: count + 1 });
        }, thumbnailRetryDelay(count));
      }}
    />
  );
}
