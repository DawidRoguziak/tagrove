import { UiButton } from "../UI/UiButton";
import { UiIconButton } from "../UI/UiIconButton";
import { browserAssistDisabledProps } from "../UI/inputBehavior";
import { useTranslation } from "react-i18next";

interface MediaGroupSetterProps {
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
  groupKey,
  groupOrder,
  onGroupKeyChange,
  onGroupOrderChange,
  onApply
}: MediaGroupSetterProps) {
  const { t } = useTranslation();

  return (
    <div className="grid gap-2">
      <div className="grid grid-cols-[minmax(0,7fr)_40px_minmax(88px,2fr)] items-center gap-2">
        <input
          id="lightbox-media-group-key-input"
          value={groupKey}
          {...browserAssistDisabledProps}
          onChange={(event) => onGroupKeyChange(event.target.value)}
          placeholder={t("lightbox.mediaGroupKeyPlaceholder")}
          aria-label={t("lightbox.mediaGroupKeyAria")}
          className="h-10 w-full"
        />

        <UiIconButton
          icon="reset"
          aria-label={t("lightbox.generateGroupAria")}
          title={t("bulk.groupModal.generateUuid")}
          onClick={() => onGroupKeyChange(generateUuid())}
        />

        <input
          id="lightbox-media-group-order-input"
          type="number"
          step="any"
          value={groupOrder}
          {...browserAssistDisabledProps}
          onChange={(event) => onGroupOrderChange(event.target.value)}
          placeholder={t("lightbox.mediaGroupOrderPlaceholder")}
          aria-label={t("lightbox.mediaGroupOrderAria")}
          className="h-10 w-full"
        />
      </div>

      <UiButton variant="primary" type="button" className="h-9 min-h-9 justify-center" onClick={onApply}>
        {t("common.apply")}
      </UiButton>
    </div>
  );
}
