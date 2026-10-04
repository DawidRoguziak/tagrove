import { render } from "@solidjs/testing-library";
import { createSignal, flush, type Component } from "solid-js";
import { LanguageProvider } from "../i18n/solid";

/** Update props on the same owned component, preserving its local state and DOM. */
export function renderSolid<P extends object>(Component: Component<P>, initial: NoInfer<P>) {
  let update = (_next: P) => {};
  const view = render(() => {
    const [props, setProps] = createSignal<P>(() => initial);
    update = (next) => {
      setProps(() => next);
      flush();
    };
    return (
      <LanguageProvider>
        <Component {...props()} />
      </LanguageProvider>
    );
  });
  flush();
  return { ...view, rerender: update };
}
