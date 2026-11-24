//! The settings page (`createSettings` and its sections in `src/features/settings`).
//!
//! - [`model`]: the [`Settings`] model the runtime embeds: the exclusive operation
//!   runner and the scan, thumbnail, import/export, duplicate and danger-zone
//!   operations built on it, the confirmations and the status of the video tools.
//! - [`transfer`]: the file dialogs of import and export, the default names of
//!   exported files and what those operations report
//!   (`createImportExportSettings.ts`).
//! - [`duplicates`]: the rules a batch of duplicate changes must meet and the form
//!   of the resolver dialog (`duplicateValidationService.ts`,
//!   `createDuplicateResolver.ts`).
//! - [`status`]: the status line of a section, kept untranslated until it is shown, so
//!   a stored message follows the language (`statusMessage.ts`,
//!   `operationStateService.ts`).
//! - [`progress`]: which progress phases belong to which operation and how a phase
//!   reads (`progressService.ts`).
//! - [`scan`]: the folder picker's selection and the texts of scan results
//!   (`createScanSettings.ts`), and the roots the scan section lists.
//! - [`navigation`]: the sections of the page in order, the section the scroll
//!   position is in, and where a click on the navigation scrolls to
//!   (`SettingsNavigation.tsx`).
//! - [`about`]: the build metadata, the links and the legal texts of the About
//!   section.
//! - [`layout`]: the measures of the page from `src/styles.css` and the Tailwind
//!   classes of the sections.
//!
//! Contract: `docs/subsystems/settings-operations.md` ("GPUI port: settings").

pub mod about;
pub mod duplicates;
pub mod layout;
pub mod model;
pub mod navigation;
pub mod progress;
pub mod scan;
pub mod status;
pub mod transfer;

pub use duplicates::{DuplicateAction, DuplicateChange, DuplicateResolver};
pub use model::{
    FolderPickerRequest, OperationId, Settings, SettingsCall, SettingsCommand, SettingsEffect,
    SettingsInput, SettingsReply, SETTINGS_TICKET_BASE,
};
pub use navigation::SettingsSection;
pub use status::{OperationSection, ProgressEvent, SectionStatus, StatusMessage};
pub use transfer::{DialogTitle, PathDialogKind, PathDialogRequest};
