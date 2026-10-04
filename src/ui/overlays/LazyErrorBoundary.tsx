import { Errored, Show } from "solid-js";
import type { JSX } from "@solidjs/web";
export function LazyErrorBoundary(props: {
  resetKey: string | number | boolean | null;
  fallback: JSX.Element;
  children: JSX.Element;
}) {
  // Key by an object so null/false remain valid keys. A failed import still needs reload.
  return (
    <Show when={{ key: props.resetKey }} keyed>
      {(_key) => (
        <Errored
          fallback={(error) => {
            console.error("Lazy UI chunk failed", error());
            return props.fallback;
          }}
        >
          {props.children}
        </Errored>
      )}
    </Show>
  );
}
