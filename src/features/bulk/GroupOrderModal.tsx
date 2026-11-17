import { createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { useLanguage } from "../../i18n/solid";
import type { BulkController, BulkGroupSaveResult } from "./createBulk";
import { UiAlert } from "../../ui/controls/UiAlert";
import { UiButton } from "../../ui/controls/UiButton";
import { createModalSnapshot } from "../../ui/overlays/createModalPresence";
import { UiModal } from "../../ui/overlays/UiModal";
import { GroupOrderBoard } from "./GroupOrderBoard";

/** The "larger view" of group ordering. It starts from `initialOrder` and keeps its own draft. */
export function GroupOrderModal(props: {
  open: boolean;
  controller: BulkController;
  initialOrder: readonly number[];
  onClose: () => void;
}) {
  const { t } = useLanguage();
  const open = createMemo(() => props.open);
  const [order, setOrder] = createSignal<number[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [result, setResult] = createSignal<BulkGroupSaveResult | null>(null);
  let savePending = false;
  let session = 0;
  let mounted = true;
  onCleanup(() => {
    mounted = false;
  });
  createEffect(open, (open) => {
    session++;
    if (!open) return;
    savePending = false;
    setSaving(false);
    setOrder([...props.initialOrder]);
    setResult(null);
  });
  const visible = createModalSnapshot(open, () => ({
    assets: props.controller.selectedAssets,
    key: props.controller.groupKeyDraft
  }));
  const locked = () => saving() || props.controller.groupApplying;

  const save = async () => {
    if (savePending || locked()) return;
    const capturedSession = session;
    savePending = true;
    setSaving(true);
    setResult(null);
    try {
      const outcome = await props.controller.onApplyGroup(order());
      if (!mounted || session !== capturedSession) return;
      setResult(outcome);
      if (outcome.status === "saved") props.onClose();
    } catch {
      if (mounted && session === capturedSession) setResult({ status: "failed" });
    } finally {
      if (mounted && session === capturedSession) {
        savePending = false;
        setSaving(false);
      }
    }
  };

  return (
    <UiModal
      open={open()}
      onClose={() => {
        if (!savePending && !locked()) props.onClose();
      }}
      size="xlarge"
      closeOnEscape={!locked()}
      closeOnOverlayClick={!locked()}
      ariaLabel={t("bulk.orderModal.title")}
      testId="bulk-order-modal"
      contentClassName="flex h-[90vh] min-h-0 flex-col gap-4 overflow-hidden"
    >
      <header class="shrink-0">
        <h2 class="m-0 text-lg font-semibold">{t("bulk.orderModal.title")}</h2>
        <p class="m-0 truncate text-sm text-[var(--text-muted)]" title={visible().key}>
          {visible().key} · {t("bulk.panel.selectedItems", { count: order().length })}
        </p>
        <p class="mt-1 text-xs text-[var(--text-muted)]">{t("bulk.orderModal.instructions")}</p>
      </header>
      <GroupOrderBoard
        active={open()}
        order={order()}
        onOrderChange={setOrder}
        assets={visible().assets}
        locked={locked()}
        minTileWidth={280}
        testIdPrefix="bulk-order"
        class="flex-1"
      />
      <footer class="grid shrink-0 gap-3 border-t border-[var(--border-soft)] pt-4">
        {result()?.status === "failed" || result()?.status === "ignored" ? (
          <UiAlert tone="error" title={t("bulk.panel.saveFailedTitle")}>
            {t("bulk.panel.groupSaveFailed")}
          </UiAlert>
        ) : result()?.status === "partial" ? (
          <UiAlert tone="warning" title={t("bulk.panel.partialResult", result() ?? {})}>
            {t("bulk.orderModal.partial")}
          </UiAlert>
        ) : null}
        <div class="flex justify-end gap-2">
          <UiButton disabled={locked()} onClick={props.onClose}>
            {t("common.cancel")}
          </UiButton>
          <UiButton
            variant="primary"
            disabled={
              locked() ||
              props.controller.selectionBusy ||
              props.controller.metadataLoading ||
              props.controller.metadataFailed
            }
            aria-busy={saving() ? "true" : "false"}
            onClick={() => void save()}
          >
            {saving() ? t("bulk.groupModal.applyInProgress") : t("bulk.orderModal.save")}
          </UiButton>
        </div>
      </footer>
    </UiModal>
  );
}
