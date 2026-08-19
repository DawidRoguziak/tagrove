import { useEffect, type ReactNode } from "react";
import { createPortal } from "react-dom";

export type UiModalSize = "small" | "medium" | "large" | "xlarge";

interface UiModalProps {
  open: boolean;
  onClose: () => void;
  closeOnOverlayClick?: boolean;
  closeOnEscape?: boolean;
  size?: UiModalSize;
  ariaLabel?: string;
  labelledBy?: string;
  className?: string;
  contentClassName?: string;
  testId?: string;
  children: ReactNode;
}

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
  className = "",
  contentClassName = "",
  testId,
  children
}: UiModalProps) {
  useEffect(() => {
    if (!open || !closeOnEscape) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [closeOnEscape, onClose, open]);

  if (!open) {
    return null;
  }

  const overlayClasses = [
    "fixed inset-0 z-[60] grid place-items-center bg-neutral/58 p-3 backdrop-blur-sm sm:p-5",
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
      role="dialog"
      aria-modal="true"
      aria-label={ariaLabel}
      aria-labelledby={labelledBy}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && closeOnOverlayClick) {
          onClose();
        }
      }}
      data-testid={testId}
    >
      <div className={contentClasses} onMouseDown={(event) => event.stopPropagation()}>
        {children}
      </div>
    </div>,
    document.body
  );
}
