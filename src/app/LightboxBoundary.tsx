import { Loading } from "solid-js";
import type { JSX } from "@solidjs/web";
import { useLanguage } from "../i18n/solid";
import { UiModal } from "../ui/overlays/UiModal";
import { LazyErrorBoundary } from "../ui/overlays/LazyErrorBoundary";
import { UiAlert } from "../ui/controls/UiAlert";
import { UiButton } from "../ui/controls/UiButton";

// Stage 6 supplies the lazy media view. Keep loading and failure modality here.
export default function LightboxBoundary(props: {
  open: boolean;
  name: string;
  resetKey: number;
  onClose: () => void;
  getRestoreFocus?: () => HTMLElement | null;
  children: JSX.Element;
}) {
  const { t } = useLanguage();
  return (
    <LazyErrorBoundary
      resetKey={props.resetKey}
      fallback={
        <UiModal
          open={props.open}
          onClose={props.onClose}
          getRestoreFocus={props.getRestoreFocus}
          ariaLabel={t("lazyLoad.lightboxFailed")}
          size="small"
        >
          <UiAlert tone="error" title={t("lazyLoad.lightboxFailed")}>
            {t("lazyLoad.retryDescription")}
          </UiAlert>
          <div class="mt-4 flex gap-2">
            <UiButton onClick={() => window.location.reload()}>{t("gallery.retry")}</UiButton>
            <UiButton variant="ghost" onClick={props.onClose}>
              {t("common.close")}
            </UiButton>
          </div>
        </UiModal>
      }
    >
      <Loading
        fallback={
          <UiModal
            open={props.open}
            onClose={props.onClose}
            ariaLabel={t("lightbox.previewDialog", { name: props.name })}
            size="small"
          >
            <p class="m-0 text-center" role="status">
              {t("common.loading")}
            </p>
          </UiModal>
        }
      >
        {props.children}
      </Loading>
    </LazyErrorBoundary>
  );
}
