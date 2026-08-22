import { useEffect, useRef, useState } from "react";
import { getVideoStreamUrl } from "../../../api";

interface VideoSourceState {
  key: string;
  src: string | null;
  failed: boolean;
}

export function useLightboxVideoSource(assetId: number, path: string | null, isVideo: boolean) {
  const requestGenerationRef = useRef(0);
  const ready = isVideo && path !== null;
  const key = ready ? `${assetId}\u0000${path}` : "";
  const [state, setState] = useState<VideoSourceState>({ key: "", src: null, failed: false });

  useEffect(() => {
    const generation = ++requestGenerationRef.current;
    setState({ key, src: null, failed: false });
    if (!ready) {
      return () => {
        requestGenerationRef.current += 1;
      };
    }

    void getVideoStreamUrl(assetId).then(
      (src) => {
        if (requestGenerationRef.current !== generation) {
          return;
        }
        if (!src) {
          console.error("[lightbox] Video stream URL was empty", { assetId, path });
          setState({ key, src: null, failed: true });
          return;
        }
        setState({ key, src, failed: false });
      },
      (error: unknown) => {
        if (requestGenerationRef.current !== generation) {
          return;
        }
        console.error("[lightbox] Failed to resolve video stream URL", { assetId, path, error });
        setState({ key, src: null, failed: true });
      }
    );

    return () => {
      requestGenerationRef.current += 1;
    };
  }, [assetId, key, path, ready]);

  const isCurrent = state.key === key && key !== "";
  return {
    src: isCurrent ? state.src : null,
    failed: isCurrent && state.failed,
    loading: ready && (!isCurrent || (!state.src && !state.failed))
  };
}
