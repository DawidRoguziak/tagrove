import { useState } from "react";
import { UiIconButton } from "../UI/UiIconButton";
import { TagListModal } from "./TagListModal";
import { useTranslation } from "react-i18next";

interface TagListSearchLauncherProps {
  className?: string;
  knownTags?: string[];
  onApplySearch?: (filterInput: string) => Promise<void> | void;
}

export function TagListSearchLauncher({
  className = "",
  knownTags = [],
  onApplySearch
}: TagListSearchLauncherProps) {
  const { t } = useTranslation();
  const [isOpen, setIsOpen] = useState(false);

  return (
    <>
      <UiIconButton
        icon="tag"
        className={className}
        onClick={() => setIsOpen(true)}
        aria-label={t("tagList.open")}
        title={t("tagList.open")}
      />

      <TagListModal
        open={isOpen}
        knownTags={knownTags}
        onClose={() => setIsOpen(false)}
        onApplySearch={onApplySearch}
      />
    </>
  );
}
