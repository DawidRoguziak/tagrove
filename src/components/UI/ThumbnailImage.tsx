import { useState, type ImgHTMLAttributes } from "react";

export const TRANSPARENT_THUMBNAIL_SRC =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

type ThumbnailImageProps = Omit<ImgHTMLAttributes<HTMLImageElement>, "src" | "onError"> & {
  src: string;
};

function ThumbnailImageWithFallback({ src, ...props }: ThumbnailImageProps) {
  const [loadFailed, setLoadFailed] = useState(false);

  return (
    <img
      {...props}
      src={loadFailed ? TRANSPARENT_THUMBNAIL_SRC : src}
      onError={() => setLoadFailed(true)}
    />
  );
}

export function ThumbnailImage(props: ThumbnailImageProps) {
  return <ThumbnailImageWithFallback key={props.src} {...props} />;
}
