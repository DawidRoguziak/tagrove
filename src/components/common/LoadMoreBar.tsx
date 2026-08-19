import { UiButton } from "../UI/UiButton";
import { useTranslation } from "react-i18next";

interface LoadMoreBarProps {
  onLoadMore: () => void;
  disabled: boolean;
}

export function LoadMoreBar({ onLoadMore, disabled }: LoadMoreBarProps) {
  const { t } = useTranslation();

  return (
    <section className="flex items-center justify-center p-3">
      <UiButton onClick={onLoadMore} disabled={disabled}>
        {t("controls.loadMore")}
      </UiButton>
    </section>
  );
}
