import { useEffect, useRef, useState } from "react";
import { UiButton } from "../UI/UiButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";
import { UiAlert } from "../UI/UiAlert";
import { UiModal } from "../UI/UiModal";

interface LightboxDeleteConfirmDialogProps {
  open: boolean;
  isSubmitting: boolean;
  errorMessage?: string | null;
  onClose: () => void;
  onConfirm: () => void;
}

export function LightboxDeleteConfirmDialog({
  open,
  isSubmitting,
  errorMessage,
  onClose,
  onConfirm
}: LightboxDeleteConfirmDialogProps) {
  const { t } = useTranslation();
  const [confirmationText, setConfirmationText] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) {
      setConfirmationText("");
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      inputRef.current?.focus();
    });

    return () => {
      window.cancelAnimationFrame(frame);
    };
  }, [open]);

  const confirmWord = t("lightbox.deleteConfirm.confirmWord");
  const canConfirm =
    confirmationText.trim().localeCompare(confirmWord.trim(), undefined, { sensitivity: "base" }) === 0;

  return (
    <UiModal
      open={open}
      onClose={onClose}
      closeOnEscape={!isSubmitting}
      closeOnOverlayClick={!isSubmitting}
      size="small"
      labelledBy="lightbox-delete-confirm-heading"
      describedBy="lightbox-delete-confirm-description"
      contentClassName="grid gap-2.5"
      testId="lightbox-delete-confirm-dialog"
    >
        <h3 id="lightbox-delete-confirm-heading" className="m-0">{t("lightbox.deleteConfirm.heading")}</h3>
        <p id="lightbox-delete-confirm-description" className="m-0 text-[13px] leading-[1.4] text-base-content/65">
          {t("lightbox.deleteConfirm.description")}
        </p>
        {errorMessage ? (
          <UiAlert tone="error">
            {t("lightbox.deleteConfirm.failedMetadataPreserved", { error: errorMessage })}
          </UiAlert>
        ) : null}

        <div className="grid gap-1">
          <label className="text-xs text-base-content/65" htmlFor="lightbox-delete-confirm-input">
            {t("lightbox.deleteConfirm.typeYesLabel", { value: confirmWord })}
          </label>
          <input
            id="lightbox-delete-confirm-input"
            ref={inputRef}
            value={confirmationText}
            {...browserAssistDisabledProps}
            onChange={(event) => setConfirmationText(event.currentTarget.value)}
            className="input input-sm h-10 w-full rounded-xl border border-base-content/20 bg-base-100/70"
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
    </UiModal>
  );
}
