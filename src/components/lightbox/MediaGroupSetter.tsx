import { UiButton } from "../UI/UiButton";
import { UiIconButton } from "../UI/UiIconButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";

import { parseMediaGroupOrder } from "./services/parseMediaGroupOrder";

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
  if (typeof globalThis.crypto !== "undefined" && typeof globalThis.crypto.randomUUID === "function") {
    return globalThis.crypto.randomUUID();
  }

  return `group-${Date.now()}-${Math.floor(Math.random() * 1000000)}`;
}

export function MediaGroupSetter({
  pending = false,
  groupKey,
  groupCopyConfirmed,
  canCopyMediaGroup,
  copyMediaGroupTitle,
  onCopyMediaGroup,
  groupOrder,
  onGroupKeyChange,
  onGroupOrderChange,
  onApply
}: MediaGroupSetterProps) {
  const { t } = useTranslation();

  return (
    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5">
      <div className="grid grid-cols-[minmax(0,1fr)_32px_32px] items-center gap-1.5">
        <input
          id="lightbox-media-group-key-input"
          value={groupKey}
          {...browserAssistDisabledProps}
          onChange={(event) => onGroupKeyChange(event.target.value)}
          placeholder={t("lightbox.mediaGroupKeyPlaceholder")}
          aria-label={t("lightbox.mediaGroupKeyAria")}
          className="h-8 min-w-0 w-full"
        />

        <UiIconButton
          icon={groupCopyConfirmed ? "check-square" : "copy"}
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8! justify-self-center"
          active={groupCopyConfirmed}
          disabled={!canCopyMediaGroup}
          aria-label={copyMediaGroupTitle}
          title={copyMediaGroupTitle}
          onClick={onCopyMediaGroup}
        />
        <UiIconButton
          icon="reset"
          iconClassName="h-4 w-4 shrink-0"
          className="h-8! w-8! min-h-8!"
          aria-label={t("lightbox.generateGroupAria")}
          title={t("bulk.groupModal.generateUuid")}
          onClick={() => onGroupKeyChange(generateUuid())}
        />
      </div>

      <input
        id="lightbox-media-group-order-input"
        type="text"
        inputMode="numeric"
        value={groupOrder}
        {...browserAssistDisabledProps}
        onChange={(event) => {
          const value = event.target.value;
          if (parseMediaGroupOrder(value) !== undefined) onGroupOrderChange(value);
        }}
        onPaste={(event) => {
          const input = event.currentTarget;
          const pasted = event.clipboardData.getData("text");
          const next = groupOrder.slice(0, input.selectionStart ?? 0)
            + pasted + groupOrder.slice(input.selectionEnd ?? groupOrder.length);
          if (parseMediaGroupOrder(next) === undefined) event.preventDefault();
        }}
        placeholder={t("lightbox.mediaGroupOrderPlaceholder")}
        aria-label={t("lightbox.mediaGroupOrderAria")}
        className="h-8 min-w-0 w-full"
      />

      <UiButton variant="primary" type="button" className="h-8! min-h-8! justify-center text-xs" onClick={onApply} disabled={pending || parseMediaGroupOrder(groupOrder) === undefined} aria-busy={pending}>
        {t("common.apply")}
      </UiButton>
    </div>
  );
}
