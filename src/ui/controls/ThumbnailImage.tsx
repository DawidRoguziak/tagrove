import { createSignal, omit, onSettled, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
type Props = Omit<JSX.ImgHTMLAttributes<HTMLImageElement>, "src" | "onError" | "onLoad"> & {
  src: string;
  fadeIn?: boolean;
  className?: string;
  onLoad?: (event: Event & { currentTarget: HTMLImageElement }) => void;
};
function ImageSource(props: Props) {
  const rest = omit(props, "src", "fadeIn", "className", "onLoad");
  let image: HTMLImageElement | undefined;
  const [loaded, setLoaded] = createSignal(false);
  const [failed, setFailed] = createSignal(false);
  const check = () => {
    if (image?.complete && image.naturalWidth > 0) setLoaded(true);
  };
  onSettled(check);
  const ready = () => loaded() && !failed() && props.src !== TRANSPARENT_THUMBNAIL_SRC;
  return (
    <img
      {...rest}
      crossorigin="anonymous"
      ref={(el) => {
        image = el;
      }}
      class={
        props.fadeIn
          ? `${props.className ?? ""} thumbnail-fade-in${ready() ? " thumbnail-fade-in-ready" : ""}`
          : props.className
      }
      src={failed() ? TRANSPARENT_THUMBNAIL_SRC : props.src}
      onLoad={(event) => {
        if (event.currentTarget.naturalWidth > 0) setLoaded(true);
        props.onLoad?.(event);
      }}
      onError={() => setFailed(true)}
    />
  );
}
export function ThumbnailImage(props: Props) {
  return (
    <Show keyed when={props.src}>
      {(src) => <ImageSource {...props} src={src} />}
    </Show>
  );
}
