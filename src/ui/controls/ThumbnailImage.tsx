import { createEffect, createSignal, omit } from "solid-js";
import type { JSX } from "@solidjs/web";
export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
type Props = Omit<JSX.ImgHTMLAttributes<HTMLImageElement>, "src" | "onError" | "onLoad"> & {
  src: string;
  fadeIn?: boolean;
  className?: string;
  onLoad?: (event: Event & { currentTarget: HTMLImageElement }) => void;
};
/**
 * One `<img>` for the component's lifetime. A new `src` replaces the attribute; the
 * previous picture stays hidden until the new one has loaded, so recycled gallery
 * cells never show another asset's thumbnail.
 */
export function ThumbnailImage(props: Props) {
  const rest = omit(props, "src", "fadeIn", "className", "onLoad");
  let image: HTMLImageElement | undefined;
  // Load and failure state belong to the source they were observed for.
  const [loaded, setLoaded] = createSignal<string | null>(null);
  const [failed, setFailed] = createSignal<string | null>(null);
  const isFailed = () => failed() === props.src;
  const shown = () => (isFailed() ? TRANSPARENT_THUMBNAIL_SRC : props.src);
  const ready = () => loaded() === props.src && !isFailed() && props.src !== TRANSPARENT_THUMBNAIL_SRC;
  const hidden = () => !ready() && shown() !== TRANSPARENT_THUMBNAIL_SRC;
  // A cached picture can be complete without a load event once the attribute changes.
  createEffect(
    () => shown(),
    (src) => {
      if (image?.complete && image.naturalWidth > 0 && image.getAttribute("src") === src)
        setLoaded(props.src);
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
        if (event.currentTarget.naturalWidth > 0 && event.currentTarget.getAttribute("src") === props.src)
          setLoaded(props.src);
        props.onLoad?.(event);
      }}
      onError={(event) => {
        if (event.currentTarget.getAttribute("src") === props.src) setFailed(props.src);
      }}
    />
  );
}
