import { createEffect, Errored, untrack } from "solid-js";
import type { JSX } from "@solidjs/web";
export function LazyErrorBoundary(props: {
  resetKey: string | number | boolean | null;
  fallback: JSX.Element;
  children: JSX.Element;
}) {
  return (
    <Errored
      fallback={(error, reset) => {
        console.error("Lazy UI chunk failed", error());
        const failedAtKey = untrack(() => props.resetKey);
        createEffect(() => props.resetKey, (key) => {
          if (key !== failedAtKey) reset();
        });
        return props.fallback;
      }}
    >
      {props.children}
    </Errored>
  );
}
