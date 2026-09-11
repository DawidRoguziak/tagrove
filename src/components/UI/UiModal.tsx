import { useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { useUiLayer } from "./UiLayerProvider";

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
  ariaLabel?: string;
  labelledBy?: string;
  describedBy?: string;
  className?: string;
  contentClassName?: string;
  testId?: string;
  getRestoreFocus?: () => HTMLElement | null;
  children: ReactNode;
};

const sizeClasses: Record<UiModalSize, string> = {
  small: "w-[90vw] max-w-[clamp(20rem,88vw,28.75rem)]",
  medium: "w-[90vw] max-w-[clamp(24rem,90vw,32.5rem)]",
  large: "w-[90vw] max-w-[clamp(28rem,92vw,68rem)]",
  xlarge: "w-[96vw] max-w-[clamp(34rem,96vw,88rem)]"
};

export function UiModal({
  open,
  onClose,
  closeOnOverlayClick = true,
  closeOnEscape = true,
  size = "medium",
  ariaLabel,
  labelledBy,
  describedBy,
  className = "",
  contentClassName = "",
  testId,
  getRestoreFocus,
  children
}: UiModalProps) {
  const contentRef = useRef<HTMLDivElement | null>(null);
  const { isTopLayer, layerId } = useUiLayer({
    active: open,
    modal: true,
    containerRef: contentRef,
    closeOnEscape,
    onEscape: onClose,
    getRestoreFocus
  });

  if (!open) {
    return null;
  }

  const overlayClasses = [
    "motion-enter fixed inset-0 z-[60] grid place-items-center bg-neutral/58 p-3 sm:p-5",
    className
  ]
    .filter(Boolean)
    .join(" ");
  const contentClasses = [
    "rounded-[var(--radius-panel)] border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 shadow-[var(--shadow-modal)] sm:p-6",
    sizeClasses[size],
    contentClassName
  ]
    .filter(Boolean)
    .join(" ");

  return createPortal(
    <div
      className={overlayClasses}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && closeOnOverlayClick && isTopLayer) {
          onClose();
        }
      }}
      data-testid={testId}
      data-ui-layer={layerId}
    >
      <div
        ref={contentRef}
        className={contentClasses}
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        tabIndex={-1}
        onMouseDown={(event) => event.stopPropagation()}
      >
        {children}
      </div>
    </div>,
    document.body
  );
}
