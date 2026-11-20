//! Errors of the `Backend` port.
//!
//! The core flattens its errors to text (ADR 0034: the wording is part of the Tauri
//! wire contract). The port keeps that text unchanged and adds what a model may
//! branch on: where the error came from, never how it is worded. Display text that
//! names a locale key travels as `i18n:<key>`, the format the Solid frontend
//! translates in `src/i18n/backendText.ts`.

use std::fmt::{Display, Formatter};

use media_tagger_core::error::AppError;

/// Prefix of backend display text that names a locale key.
pub const I18N_PREFIX: &str = "i18n:";

pub type BackendResult<T> = Result<T, BackendError>;

/// Where a failed backend call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendErrorKind {
    /// A core use case refused or failed; the text is the core's.
    Rejected,
    /// The call never produced a result: its task panicked or the executor
    /// dropped it (the counterpart of Tauri's `... worker failed` errors).
    WorkerFailed,
    /// The backend has no `AppState` yet: startup is still running.
    NotReady,
}

/// Backend display text, split the way the interface shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendText<'a> {
    /// `i18n:<key>`: a key of `src/i18n/locales`.
    Key(&'a str),
    /// Free-form text, shown as it is.
    Plain(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    kind: BackendErrorKind,
    text: String,
}

impl BackendError {
    /// An error of a core use case with the core's text.
    pub fn rejected(text: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::Rejected,
            text: text.into(),
        }
    }

    /// The task of `operation` ended without a result. The text follows the
    /// Tauri commands (`scan worker failed: ...`).
    pub fn worker_failed(operation: &str, detail: &str) -> Self {
        Self {
            kind: BackendErrorKind::WorkerFailed,
            text: format!("{operation} worker failed: {detail}"),
        }
    }

    /// A call that arrived before the backend was attached to its `AppState`.
    pub fn not_ready() -> Self {
        Self {
            kind: BackendErrorKind::NotReady,
            text: format!("{I18N_PREFIX}common.loading"),
        }
    }

    pub fn kind(&self) -> BackendErrorKind {
        self.kind
    }

    /// The text exactly as the backend produced it.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The locale key of an `i18n:<key>` text. Surrounding whitespace is ignored
    /// and an empty key is no key, as in `translateBackendText`.
    pub fn i18n_key(&self) -> Option<&str> {
        i18n_key(&self.text)
    }

    /// The text as the interface shows it: a locale key or plain text.
    pub fn display_text(&self) -> BackendText<'_> {
        match self.i18n_key() {
            Some(key) => BackendText::Key(key),
            None => BackendText::Plain(&self.text),
        }
    }
}

/// The locale key of `text` when it has the form `i18n:<key>`.
pub fn i18n_key(text: &str) -> Option<&str> {
    let key = text.trim().strip_prefix(I18N_PREFIX)?.trim();
    (!key.is_empty()).then_some(key)
}

impl Display for BackendError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

impl std::error::Error for BackendError {}

impl From<AppError> for BackendError {
    fn from(error: AppError) -> Self {
        Self::rejected(error.to_string())
    }
}

impl From<String> for BackendError {
    fn from(text: String) -> Self {
        Self::rejected(text)
    }
}

#[cfg(test)]
mod tests {
    use media_tagger_core::error::AppError;

    use super::{i18n_key, BackendError, BackendErrorKind, BackendText};

    #[test]
    fn an_i18n_prefix_names_a_locale_key() {
        let busy = BackendError::rejected("i18n:common.libraryBusy");

        assert_eq!(busy.kind(), BackendErrorKind::Rejected);
        assert_eq!(busy.i18n_key(), Some("common.libraryBusy"));
        assert_eq!(busy.display_text(), BackendText::Key("common.libraryBusy"));
        assert_eq!(busy.text(), "i18n:common.libraryBusy");
        assert_eq!(busy.to_string(), "i18n:common.libraryBusy");
    }

    #[test]
    fn the_key_is_trimmed_and_an_empty_key_is_plain_text() {
        assert_eq!(
            i18n_key("  i18n: common.loading \n"),
            Some("common.loading")
        );
        assert_eq!(i18n_key("i18n:"), None);
        assert_eq!(i18n_key("i18n:   "), None);
        assert_eq!(i18n_key("I18N:common.loading"), None);
        assert_eq!(i18n_key("error i18n:common.loading"), None);
    }

    #[test]
    fn free_form_core_text_stays_unchanged() {
        let error = BackendError::from(AppError::from(
            "Folder path does not exist or is not a directory",
        ));

        assert_eq!(error.kind(), BackendErrorKind::Rejected);
        assert_eq!(error.i18n_key(), None);
        assert_eq!(
            error.display_text(),
            BackendText::Plain("Folder path does not exist or is not a directory")
        );
    }

    #[test]
    fn client_side_failures_carry_their_kind() {
        let failed = BackendError::worker_failed("scan", "task panicked");
        assert_eq!(failed.kind(), BackendErrorKind::WorkerFailed);
        assert_eq!(failed.text(), "scan worker failed: task panicked");

        let starting = BackendError::not_ready();
        assert_eq!(starting.kind(), BackendErrorKind::NotReady);
        assert_eq!(starting.i18n_key(), Some("common.loading"));
    }
}
