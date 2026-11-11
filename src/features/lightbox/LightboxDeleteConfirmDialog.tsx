import { createSignal, createEffect, Show } from "solid-js";
import { UiButton } from "../../ui/controls/UiButton";
import { useLanguage } from "../../i18n/solid";
import { translateBackendText } from "../../i18n/backendText";
import { UiAlert } from "../../ui/controls/UiAlert";

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

export function LightboxDeleteConfirmDialog(props: LightboxDeleteConfirmDialogProps) {
  const { t, language } = useLanguage();
  const [confirmationText, setConfirmationText] = createSignal("");
  let input: HTMLInputElement | undefined;
  let wasOpen = false,
    focused = false;
  createEffect(
    () => ({ open: props.open, ready: props.focusReady ?? true }),
    ({ open, ready }) => {
      if (!open) {
        setConfirmationText("");
        if (wasOpen)
          document
            .getElementById(props.restoreFocusId ?? DELETE_BUTTON_ID)
            ?.focus({ preventScroll: true });
        wasOpen = false;
        focused = false;
        return;
      }
      wasOpen = true;
      // Focus the input once per open. Same-asset metadata writes re-run this effect (the
      // selection is republished) and must not pull focus away from the tag input or heart.
      if (!ready || focused) return;
      const frame = requestAnimationFrame(() => {
        focused = true;
        input?.focus();
      });
      return () => cancelAnimationFrame(frame);
    }
  );
  const confirmWord = () => t("lightbox.deleteConfirm.confirmWord");
  // Explicit locale-aware case folding: WebKitGTK's localeCompare with base sensitivity does not
  // fold case, so "yes"/"YES" would never match "Yes" in the shipped runtime.
  const fold = (value: string) => {
    const text = value.trim().normalize("NFC");
    try {
      return text.toLocaleLowerCase(language());
    } catch {
      return text.toLowerCase();
    }
  };
  const canConfirm = () => {
    const word = fold(confirmWord());
    return word.length > 0 && fold(confirmationText()) === word;
  };

  return (
    <Show when={props.open}>
      <section
        aria-label={t("lightbox.deleteConfirm.heading")}
        data-testid="lightbox-delete-confirm-dialog"
        class="motion-enter grid gap-2.5 rounded-[var(--radius-control)] border border-error/32 bg-base-200/45 p-3"
        onKeyDown={(event) => {
          if (event.key !== "Escape" || props.isSubmitting) return;
          event.preventDefault();
          event.stopPropagation();
          props.onClose();
        }}
      >
        <h3 class="m-0 text-sm">{t("lightbox.deleteConfirm.heading")}</h3>
        <p class="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("lightbox.deleteConfirm.description")}
        </p>
        {props.errorMessage ? (
          <UiAlert tone="error">
            {t("lightbox.deleteConfirm.failedMetadataPreserved", {
              error: translateBackendText(props.errorMessage, t)
            })}
          </UiAlert>
        ) : null}

        <div class="grid gap-1">
          <label class="text-xs text-[var(--text-muted)]" for="lightbox-delete-confirm-input">
            {t("lightbox.deleteConfirm.typeYesLabel", { value: confirmWord() })}
          </label>
          <input
            id="lightbox-delete-confirm-input"
            ref={(node) => {
              input = node;
            }}
            value={confirmationText()}
            autocapitalize="none"
            autocomplete="off"
            autocorrect="off"
            spellcheck={false}
            onInput={(event) => setConfirmationText(event.currentTarget.value)}
            class="input input-sm h-10 w-full rounded-[var(--radius-control)] border border-base-content/20 bg-base-100/70"
            placeholder={t("lightbox.deleteConfirm.typeYesPlaceholder", { value: confirmWord() })}
            disabled={props.isSubmitting}
          />
        </div>

        <div class="flex items-center gap-2">
          <UiButton onClick={props.onClose} disabled={props.isSubmitting}>
            {t("common.cancel")}
          </UiButton>
          <UiButton
            variant="danger"
            onClick={props.onConfirm}
            disabled={props.isSubmitting || !canConfirm()}
          >
            {t("lightbox.deleteConfirm.confirm")}
          </UiButton>
        </div>
      </section>
    </Show>
  );
}
