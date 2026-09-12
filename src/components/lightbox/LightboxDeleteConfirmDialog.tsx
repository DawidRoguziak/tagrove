import { useEffect, useRef, useState } from "react";
import { UiButton } from "../UI/UiButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";
import { UiAlert } from "../UI/UiAlert";

interface LightboxDeleteConfirmDialogProps {
  open: boolean;
  focusReady?: boolean;
  restoreFocusId?: string;
  isSubmitting: boolean;
  errorMessage?: string | null;
  onClose: () => void;
  onConfirm: () => void;
}

const DELETE_BUTTON_ID = "lightbox-delete-button";

export function LightboxDeleteConfirmDialog({
  open,
  focusReady = true,
  restoreFocusId = DELETE_BUTTON_ID,
  isSubmitting,
  errorMessage,
  onClose,
  onConfirm
}: LightboxDeleteConfirmDialogProps) {
  const { t } = useTranslation();
  const [confirmationText, setConfirmationText] = useState("");
  const wasOpenRef = useRef(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) {
      setConfirmationText("");
      return;
    }

    if (!focusReady) return;
    const frame = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
    });

    return () => {
      window.cancelAnimationFrame(frame);
    };
  }, [open, focusReady]);

  useEffect(() => {
    const wasOpen = wasOpenRef.current;
    wasOpenRef.current = open;
    if (open || !wasOpen) return;
    document.getElementById(restoreFocusId)?.focus({ preventScroll: true });
  }, [open, restoreFocusId]);

  if (!open) return null;

  const confirmWord = t("lightbox.deleteConfirm.confirmWord");
  const canConfirm =
    confirmationText.trim().localeCompare(confirmWord.trim(), undefined, { sensitivity: "base" }) === 0;

  return (
    <section
      aria-label={t("lightbox.deleteConfirm.heading")}
      data-testid="lightbox-delete-confirm-dialog"
      className="motion-enter grid gap-2.5 rounded-[var(--radius-control)] border border-error/32 bg-base-200/45 p-3"
      onKeyDown={(event) => {
        if (event.key !== "Escape" || isSubmitting) return;
        event.preventDefault();
        event.stopPropagation();
        onClose();
      }}
    >
      <h3 className="m-0 text-sm">{t("lightbox.deleteConfirm.heading")}</h3>
      <p className="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
        {t("lightbox.deleteConfirm.description")}
      </p>
      {errorMessage ? (
        <UiAlert tone="error">
          {t("lightbox.deleteConfirm.failedMetadataPreserved", { error: errorMessage })}
        </UiAlert>
      ) : null}

      <div className="grid gap-1">
        <label className="text-xs text-[var(--text-muted)]" htmlFor="lightbox-delete-confirm-input">
          {t("lightbox.deleteConfirm.typeYesLabel", { value: confirmWord })}
        </label>
        <input
          id="lightbox-delete-confirm-input"
          ref={inputRef}
          value={confirmationText}
          {...browserAssistDisabledProps}
          onChange={(event) => setConfirmationText(event.currentTarget.value)}
          className="input input-sm h-10 w-full rounded-[var(--radius-control)] border border-base-content/20 bg-base-100/70"
          placeholder={t("lightbox.deleteConfirm.typeYesPlaceholder", { value: confirmWord })}
          disabled={isSubmitting}
        />
      </div>

      <div className="flex items-center gap-2">
        <UiButton onClick={onClose} disabled={isSubmitting}>
          {t("common.cancel")}
        </UiButton>
        <UiButton variant="danger" onClick={onConfirm} disabled={isSubmitting || !canConfirm}>
          {t("lightbox.deleteConfirm.confirm")}
        </UiButton>
      </div>
    </section>
  );
}