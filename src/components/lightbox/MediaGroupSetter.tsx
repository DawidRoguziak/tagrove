import { UiButton } from "../UI/UiButton";
import { UiIconButton } from "../UI/UiIconButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";

interface MediaGroupSetterProps {
  pending?: boolean;
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
  groupOrder,
  onGroupKeyChange,
  onGroupOrderChange,
  onApply
}: MediaGroupSetterProps) {
  const { t } = useTranslation();

  return (
    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)] gap-1.5">
      <div className="grid grid-cols-[minmax(0,1fr)_32px] items-center gap-1.5">
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
        type="number"
        step="any"
        value={groupOrder}
        {...browserAssistDisabledProps}
        onChange={(event) => onGroupOrderChange(event.target.value)}
        placeholder={t("lightbox.mediaGroupOrderPlaceholder")}
        aria-label={t("lightbox.mediaGroupOrderAria")}
        className="h-8 min-w-0 w-full"
      />

      <UiButton variant="primary" type="button" className="h-8! min-h-8! justify-center text-xs" onClick={onApply} disabled={pending} aria-busy={pending}>
        {t("common.apply")}
      </UiButton>
    </div>
  );
}
