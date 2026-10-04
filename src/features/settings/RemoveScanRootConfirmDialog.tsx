import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { UiButton } from "../../ui/controls/UiButton";
import { UiModal } from "../../ui/overlays/UiModal";
import { useLanguage } from "../../i18n/solid";
interface RemoveScanRootConfirmDialogProps {
  path: string | null;
  isOperationLocked: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}
export function RemoveScanRootConfirmDialog(props: RemoveScanRootConfirmDialogProps) {
  const { t } = useLanguage();
  const visible = createModalSnapshot(
    () => Boolean(props.path),
    () => ({ path: props.path, locked: props.isOperationLocked })
  );
  return (
    <UiModal
      open={Boolean(props.path)}
      onClose={props.onCancel}
      closeOnOverlayClick={!props.isOperationLocked}
      closeOnEscape={!props.isOperationLocked}
      size="medium"
      labelledBy="remove-scan-root-confirm-heading"
    >
      <div class="grid gap-2.5">
        <h3 id="remove-scan-root-confirm-heading" class="m-0">
          {t("settings.dialogs.removeScanRoot.heading")}
        </h3>
        <p class="m-0 text-[13px] leading-[1.4] text-[var(--text-muted)]">
          {t("settings.dialogs.removeScanRoot.description")}
        </p>
        <p class="m-0 break-all rounded-[var(--radius-control)] border border-base-content/20 bg-base-200/50 px-2.5 py-2 text-xs text-[var(--text-muted)]">
          {visible().path}
        </p>
        <div class="flex items-center gap-2">
          <UiButton variant="danger" onClick={props.onConfirm} disabled={visible().locked}>
            {t("common.remove")}
          </UiButton>
          <UiButton onClick={props.onCancel} disabled={visible().locked}>
            {t("common.cancel")}
          </UiButton>
        </div>
      </div>
    </UiModal>
  );
}
