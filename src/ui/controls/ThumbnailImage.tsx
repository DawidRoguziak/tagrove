import { createEffect, createSignal, omit, onCleanup } from "solid-js";
import type { JSX } from "@solidjs/web";
export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
/** Delays before each retry of a failed load; their count is the retry budget. */
export const THUMBNAIL_RETRY_DELAYS_MS = [500, 2000] as const;
type Props = Omit<JSX.ImgHTMLAttributes<HTMLImageElement>, "src" | "onError" | "onLoad"> & {
  src: string;
  /** URL for retry `attempt` (1-based) after a failed load; without it a failure is final. */
  retrySrc?: (attempt: number) => string;
  fadeIn?: boolean;
  className?: string;
  onLoad?: (event: Event & { currentTarget: HTMLImageElement }) => void;
  /** Called when the picture of the current `src` (or one of its retries) is shown. */
  onReady?: () => void;
  /** Called once per `src` after the load and every retry failed. */
  onFailed?: () => void;
};
/**
 * One `<img>` for the component's lifetime. A new `src` replaces the attribute; the
 * previous picture stays hidden until the new one has loaded, so recycled gallery
 * cells never show another asset's thumbnail. A failed load is retried with backoff
 * through `retrySrc` (a 503 or a render that lost a race), then shows a transparent
 * placeholder.
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
  const [failed, setFailed] = createSignal<string | null>(null);
  const [attempt, setAttempt] = createSignal<{ src: string; count: number } | null>(null);
  const attemptCount = () => {
    const current = attempt();
    return current && current.src === props.src ? current.count : 0;
  };
  /** The URL actually requested for the current `src`. */
  const requested = () => {
    const count = attemptCount();
    return count > 0 && props.retrySrc ? props.retrySrc(count) : props.src;
  };
  const isFailed = () => failed() === props.src;
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
  // A cached picture can be complete without a load event once the attribute changes.
  createEffect(
    () => shown(),
    (src) => {
      clearRetry();
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
        const src = props.src;
        const count = attemptCount();
        if (props.retrySrc && count < THUMBNAIL_RETRY_DELAYS_MS.length) {
          clearRetry();
          retryTimer = setTimeout(() => {
            retryTimer = undefined;
            if (props.src === src) setAttempt({ src, count: count + 1 });
          }, THUMBNAIL_RETRY_DELAYS_MS[count]);
          return;
        }
        setFailed(src);
        props.onFailed?.();
      }}
    />
  );
}
