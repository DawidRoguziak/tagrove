import { UiButton } from "../../ui/controls/UiButton";
import { UiIconButton } from "../../ui/controls/UiIconButton";
import { useLanguage } from "../../i18n/solid";

import { parseMediaGroupOrder } from "./parseMediaGroupOrder";

interface MediaGroupSetterProps {
  pending?: boolean;
  groupCopyConfirmed: boolean;
  canCopyMediaGroup: boolean;
  copyMediaGroupTitle: string;
  onCopyMediaGroup: () => void;
  groupKey: string;
  groupOrder: string;
  onGroupKeyChange: (value: string) => void;
  onGroupOrderChange: (value: string) => void;
  onApply: () => void;
}

function generateUuid(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.randomUUID === "function"
  ) {
    return globalThis.crypto.randomUUID();
  }

  return `group-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
}

export function MediaGroupSetter(props: MediaGroupSetterProps) {
  const { t } = useLanguage();

  return (
    <div class="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5">
      <div class="grid grid-cols-[minmax(0,1fr)_32px_32px] items-center gap-1.5">
        <input
          id="lightbox-media-group-key-input"
          value={props.groupKey}
          autocapitalize="none"
          autocomplete="off"
          autocorrect="off"
          spellcheck={false}
          onInput={(event) => props.onGroupKeyChange(event.currentTarget.value)}
          placeholder={t("lightbox.mediaGroupKeyPlaceholder")}
          aria-label={t("lightbox.mediaGroupKeyAria")}
          class="h-8 min-w-0 w-full"
        />

        <UiIconButton
          icon={props.groupCopyConfirmed ? "check-square" : "copy"}
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={props.groupCopyConfirmed}
          disabled={!props.canCopyMediaGroup}
          aria-label={props.copyMediaGroupTitle}
          title={props.copyMediaGroupTitle}
          onClick={props.onCopyMediaGroup}
        />
        <UiIconButton
          icon="reset"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8!"
          aria-label={t("lightbox.generateGroupAria")}
          title={t("bulk.groupModal.generateUuid")}
          onClick={() => props.onGroupKeyChange(generateUuid())}
        />
      </div>

      <input
        id="lightbox-media-group-order-input"
        type="text"
        inputmode="numeric"
        value={props.groupOrder}
        autocapitalize="none"
        autocomplete="off"
        autocorrect="off"
        spellcheck={false}
        onInput={(event) => {
          const value = event.currentTarget.value;
          if (parseMediaGroupOrder(value) !== undefined) props.onGroupOrderChange(value);
          else event.currentTarget.value = props.groupOrder;
        }}
        onPaste={(event) => {
          const input = event.currentTarget;
          const pasted = event.clipboardData?.getData("text") ?? "";
          const next =
            props.groupOrder.slice(0, input.selectionStart ?? 0) +
            pasted +
            props.groupOrder.slice(input.selectionEnd ?? props.groupOrder.length);
          if (parseMediaGroupOrder(next) === undefined) event.preventDefault();
        }}
        placeholder={t("lightbox.mediaGroupOrderPlaceholder")}
        aria-label={t("lightbox.mediaGroupOrderAria")}
        class="h-8 min-w-0 w-full"
      />

      <UiButton
        variant="primary"
        type="button"
        className="h-8! min-h-8! justify-center text-xs"
        onClick={props.onApply}
        disabled={props.pending || parseMediaGroupOrder(props.groupOrder) === undefined}
        aria-busy={props.pending ? "true" : "false"}
      >
        {t("common.apply")}
      </UiButton>
    </div>
  );
}
