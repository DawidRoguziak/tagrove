import { createSignal } from "solid-js";
import { UiIconButton } from "../../../ui/controls/UiIconButton";
import { useLanguage } from "../../../i18n/solid";
import { TagListModal } from "./TagListModal";
export function TagListSearchLauncher(props: {
  className?: string;
  knownTags: string[];
  onApplySearch: (value: string) => Promise<void>;
}) {
  const { t } = useLanguage();
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <UiIconButton
        icon="tag"
        className={props.className}
        onClick={() => setOpen(true)}
        aria-label={t("tagList.open")}
        title={t("tagList.open")}
      />
      <TagListModal
        open={open()}
        knownTags={props.knownTags}
        onClose={() => setOpen(false)}
        onApplySearch={props.onApplySearch}
      />
    </>
  );
}
