//! The import/export section of the settings page (`ImportExportSection.tsx`) and the
//! overwrite confirmation of a restore (`ImportDbOverwriteConfirmDialog.tsx`).
//!
//! The four buttons only send their command: the dialogs, the mutation barrier and
//! the refreshes are the settings model's (`media_tagger_client::settings`).

use gpui::{div, prelude::*, px, relative, AnyElement, Context};
use media_tagger_client::{
    controls::ButtonVariant,
    i18n::I18n,
    overlay::ModalSize,
    runtime::RuntimeInput,
    settings::{layout, OperationSection, SettingsCommand},
    theme::ThemeName,
};

use super::{
    card, confirm_buttons, confirm_text, dialog_heading, heading, operation_row, status_block,
    SettingsView,
};
use crate::ui::{button::Button, modal::Modal, theme::color};

/// The import/export card.
pub const IMPORT_EXPORT_SECTION_SELECTOR: &str = "import-export-settings-section";
pub const IMPORT_EXPORT_LOADER_SELECTOR: &str = "import-export-section-loader";
/// Element ids of the four actions.
pub const EXPORT_CSV_ID: &str = "settings-export-csv";
pub const IMPORT_CSV_ID: &str = "settings-import-csv";
pub const EXPORT_DB_BUNDLE_ID: &str = "settings-export-db-bundle";
pub const IMPORT_DB_BUNDLE_ID: &str = "settings-import-db-bundle";
/// Element ids of the overwrite confirmation's buttons.
pub const IMPORT_OVERWRITE_CONFIRM_ID: &str = "settings-import-db-overwrite-confirm";
pub const IMPORT_OVERWRITE_CANCEL_ID: &str = "settings-import-db-overwrite-cancel";

impl SettingsView {
    pub(super) fn import_export_section(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let settings = self.runtime.read(cx).model().settings();
        let locked = settings.is_locked();
        let action = |id: &'static str, label: &str, command: SettingsCommand| {
            Button::new(id, i18n.t(label))
                .debug_selector(id)
                .disabled(locked)
                .on_click(self.send_on_click(move || RuntimeInput::settings(command.clone())))
        };
        let actions = || {
            div()
                .flex()
                .flex_none()
                .flex_wrap()
                .gap(px(layout::operation::ACTIONS_GAP))
        };
        card(theme)
            .debug_selector(|| IMPORT_EXPORT_SECTION_SELECTOR.to_owned())
            .gap(px(layout::data::GAP))
            .child(heading(i18n.t("settings.importExport.heading")))
            .child(
                operation_row(theme, i18n.t("settings.importExport.csvDescription")).child(
                    actions()
                        .child(action(
                            EXPORT_CSV_ID,
                            "settings.importExport.exportCsv",
                            SettingsCommand::ExportCsv,
                        ))
                        .child(action(
                            IMPORT_CSV_ID,
                            "settings.importExport.importCsv",
                            SettingsCommand::ImportCsv,
                        )),
                ),
            )
            .child(
                operation_row(theme, i18n.t("settings.importExport.bundleDescription")).child(
                    actions()
                        .child(action(
                            EXPORT_DB_BUNDLE_ID,
                            "settings.importExport.exportDbBundle",
                            SettingsCommand::ExportDbBundle,
                        ))
                        .child(action(
                            IMPORT_DB_BUNDLE_ID,
                            "settings.importExport.importDbBundle",
                            SettingsCommand::ImportDbBundle,
                        )),
                ),
            )
            .child(status_block(
                "settings-import-export-status",
                IMPORT_EXPORT_LOADER_SELECTOR,
                settings.status(OperationSection::ImportExport),
                i18n,
                theme,
            ))
            .child(
                div()
                    .text_size(px(layout::data::HINT_SIZE))
                    .line_height(relative(layout::data::HINT_LINE_HEIGHT))
                    .text_color(color(colors.error))
                    .child(i18n.t("settings.importExport.hint")),
            )
            .into_any_element()
    }

    /// The overwrite confirmation of a restore: nothing is imported before "Confirm".
    pub(super) fn import_overwrite_dialog(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locked = self.runtime.read(cx).model().settings().is_locked();
        Modal::new(
            "settings-import-db-overwrite-dialog",
            &self.import_overwrite_modal,
        )
        .label(i18n.t("common.confirm"))
        .size(ModalSize::Small)
        .theme(theme)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(layout::confirm::GAP))
                .child(dialog_heading(i18n.t("common.confirm")))
                .child(confirm_text(
                    theme,
                    i18n.t("settings.actions.dialogs.confirmDbImportOverwrite"),
                ))
                .child(
                    confirm_buttons()
                        .child(
                            Button::new(IMPORT_OVERWRITE_CANCEL_ID, i18n.t("common.cancel"))
                                .debug_selector(IMPORT_OVERWRITE_CANCEL_ID)
                                .disabled(locked)
                                .on_click(self.send_on_click(|| {
                                    RuntimeInput::settings(SettingsCommand::CancelImportDbOverwrite)
                                })),
                        )
                        .child(
                            Button::new(IMPORT_OVERWRITE_CONFIRM_ID, i18n.t("common.confirm"))
                                .debug_selector(IMPORT_OVERWRITE_CONFIRM_ID)
                                .variant(ButtonVariant::Danger)
                                .disabled(locked)
                                .on_click(self.send_on_click(|| {
                                    RuntimeInput::settings(
                                        SettingsCommand::ConfirmImportDbOverwrite,
                                    )
                                })),
                        ),
                ),
        )
        .into_any_element()
    }
}
