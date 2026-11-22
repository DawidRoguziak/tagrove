//! The search model (`createSearch`): the draft the header edits and the filters that
//! were applied.
//!
//! Typing changes only the draft and clears the validation message. Search, Enter, the
//! tag list's Apply, the media kind, favourites and Clear all go through one submit:
//! it parses the draft, and invalid input keeps the applied filters and shows the
//! message; valid input becomes the applied descriptor and starts the query once,
//! also when the descriptor is unchanged (a resubmit refreshes). The model answers a
//! valid submit with the [`LibraryFilters`] to apply; the runtime hands them to the
//! query.

use super::{
    filters::{build_filter_descriptor, FilterDescriptor, SearchFilters},
    grammar::{parse_search_filter, SearchFilterValidationError},
};
use crate::{backend::MediaKind, library::LibraryFilters};

/// The commands of the search header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchCommand {
    /// The field's text changed (`change`).
    Change(String),
    /// Search or Enter (`submit`).
    Submit,
    /// Replaces the draft and submits (`applyInput`, the tag list's Apply).
    ApplyInput(String),
    /// A media kind; `None` is "all" (`applyKind`).
    ApplyKind(Option<MediaKind>),
    ApplyFavorites(bool),
    /// Back to the defaults, submitted (`clear`).
    Clear,
}

#[derive(Debug, Clone, Default)]
pub struct Search {
    draft: SearchFilters,
    applied: FilterDescriptor,
    /// Counts the changes of [`Self::applied`].
    applied_version: u64,
    validation_error: Option<SearchFilterValidationError>,
}

impl Search {
    pub fn new() -> Self {
        Self::default()
    }

    /// The draft: the field's text, the media kind and favourites as shown.
    pub fn draft(&self) -> &SearchFilters {
        &self.draft
    }

    pub fn filter_input(&self) -> &str {
        &self.draft.filter_input
    }

    pub fn media_kind(&self) -> Option<MediaKind> {
        self.draft.media_kind
    }

    pub fn favorites_only(&self) -> bool {
        self.draft.favorites_only
    }

    /// The applied filters, normalized.
    pub fn applied(&self) -> &FilterDescriptor {
        &self.applied
    }

    /// Changes only when a submit applied a different descriptor (Solid publishes the
    /// applied signal only then).
    pub fn applied_version(&self) -> u64 {
        self.applied_version
    }

    /// Why the last submit was refused; cleared by every change of the draft.
    pub fn validation_error(&self) -> Option<SearchFilterValidationError> {
        self.validation_error
    }

    /// Applies `command`; a valid submit answers with the filters to apply.
    pub fn handle(&mut self, command: SearchCommand) -> Option<LibraryFilters> {
        match command {
            SearchCommand::Change(filter_input) => {
                self.update(|draft| draft.filter_input = filter_input);
                None
            }
            SearchCommand::Submit => self.submit(),
            SearchCommand::ApplyInput(filter_input) => {
                self.update(|draft| draft.filter_input = filter_input);
                self.submit()
            }
            SearchCommand::ApplyKind(media_kind) => {
                self.update(|draft| draft.media_kind = media_kind);
                self.submit()
            }
            SearchCommand::ApplyFavorites(favorites_only) => {
                self.update(|draft| draft.favorites_only = favorites_only);
                self.submit()
            }
            SearchCommand::Clear => {
                self.update(|draft| *draft = SearchFilters::default());
                self.submit()
            }
        }
    }

    fn update(&mut self, change: impl FnOnce(&mut SearchFilters)) {
        change(&mut self.draft);
        self.validation_error = None;
    }

    fn submit(&mut self) -> Option<LibraryFilters> {
        let parsed = parse_search_filter(&self.draft.filter_input);
        self.validation_error = parsed.validation_error;
        if self.validation_error.is_some() {
            return None;
        }
        let descriptor = build_filter_descriptor(&self.draft);
        if descriptor != self.applied {
            self.applied = descriptor;
            self.applied_version += 1;
        }
        Some(self.applied.library_filters())
    }
}
