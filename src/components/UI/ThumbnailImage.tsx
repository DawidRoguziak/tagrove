import { useCallback, useState, type ImgHTMLAttributes } from "react";

export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

type ThumbnailImageProps = Omit<ImgHTMLAttributes<HTMLImageElement>, "src" | "onError"> & {
  src: string;
  fadeIn?: boolean;
};

function ThumbnailImageWithFallback({ src, fadeIn = false, className, onLoad, ...props }: ThumbnailImageProps) {
  const [loadFailed, setLoadFailed] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const imageRef = useCallback((image: HTMLImageElement | null) => {
    if (fadeIn && image?.complete && image.naturalWidth > 0) setLoaded(true);
  }, [fadeIn]);
  const ready = loaded && !loadFailed && src !== TRANSPARENT_THUMBNAIL_SRC;

  return (
    <img
      {...props}
      ref={imageRef}
      className={fadeIn
        ? `${className ?? ""} thumbnail-fade-in${ready ? " thumbnail-fade-in-ready" : ""}`
        : className}
      src={loadFailed ? TRANSPARENT_THUMBNAIL_SRC : src}
      onLoad={(event) => {
        if (fadeIn && event.currentTarget.naturalWidth > 0) setLoaded(true);
        onLoad?.(event);
      }}
      onError={() => setLoadFailed(true)}
    />
  );
}

export function ThumbnailImage(props: ThumbnailImageProps) {
  return <ThumbnailImageWithFallback key={props.src} {...props} />;
}
