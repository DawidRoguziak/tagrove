import { Portal, type JSX } from "@solidjs/web";
import { Show } from "solid-js";
import { createModalPresence, createModalSnapshot } from "./createModalPresence";
import { createUiLayer } from "./UiLayerProvider";

export type UiModalSize = "small" | "medium" | "large" | "xlarge";
type UiModalName =
  | { ariaLabel: string; labelledBy?: never }
  | { ariaLabel?: never; labelledBy: string };
type UiModalProps = UiModalName & {
  open: boolean;
  onClose: () => void;
  closeOnOverlayClick?: boolean;
  closeOnEscape?: boolean;
  size?: UiModalSize;
  describedBy?: string;
  className?: string;
  contentClassName?: string;
  testId?: string;
  getRestoreFocus?: () => HTMLElement | null;
  children: JSX.Element;
};
const sizes: Record<UiModalSize, string> = {
  small: "w-[90vw] max-w-[clamp(20rem,88vw,28.75rem)]",
  medium: "w-[90vw] max-w-[clamp(24rem,90vw,32.5rem)]",
  large: "w-[90vw] max-w-[clamp(28rem,92vw,68rem)]",
  xlarge: "w-[96vw] max-w-[clamp(34rem,96vw,88rem)]"
};
export function UiModal(props: UiModalProps) {
  const presence = createModalPresence(() => props.open);
  const visible = createModalSnapshot(
    () => props.open,
    () => ({
      ariaLabel: props.ariaLabel,
      labelledBy: props.labelledBy,
      describedBy: props.describedBy,
      size: props.size ?? "medium",
      className: props.className,
      contentClassName: props.contentClassName
    })
  );
  let content: HTMLDivElement | undefined;
  const layer = createUiLayer({
    active: () => props.open,
    modal: true,
    container: () => content,
    closeOnEscape: () => props.closeOnEscape ?? true,
    onEscape: () => props.onClose(),
    getRestoreFocus: () => props.getRestoreFocus?.() ?? null
  });
  return (
    <Show when={presence.present()}>
      <Portal>
        <div
          ref={presence.overlayRef}
          data-modal-presence={presence.phase()}
          aria-hidden={!props.open ? "true" : undefined}
          class={[
            "fixed inset-0 z-[60] grid place-items-center bg-neutral/58 p-3 sm:p-5",
            visible().className
          ]
            .filter(Boolean)
            .join(" ")}
          data-testid={props.testId}
          data-ui-layer={layer.layerId}
          onMouseDown={(event) => {
            if (
              props.open &&
              event.target === event.currentTarget &&
              (props.closeOnOverlayClick ?? true) &&
              layer.isTopLayer()
            )
              props.onClose();
          }}
        >
          <div
            ref={(el) => {
              content = el;
            }}
            inert={!props.open}
            class={[
              "rounded-[var(--radius-panel)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-modal)] sm:p-6",
              sizes[visible().size],
              visible().contentClassName
            ]
              .filter(Boolean)
              .join(" ")}
            role="dialog"
            aria-modal="true"
            aria-label={visible().ariaLabel}
            aria-labelledby={visible().labelledBy}
            aria-describedby={visible().describedBy}
            tabindex={-1}
            onMouseDown={(event) => event.stopPropagation()}
          >
            {props.children}
          </div>
        </div>
      </Portal>
    </Show>
  );
}
