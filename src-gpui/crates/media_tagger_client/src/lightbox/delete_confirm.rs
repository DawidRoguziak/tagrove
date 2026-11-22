//! The inline delete confirmation of the lightbox (`LightboxDeleteConfirmDialog.tsx`
//! and the delete state of `LightboxModal.tsx`): the confirmation word that has to be
//! typed, the running deletion that locks every exit, and what the backend's answer
//! shows.
//!
//! [`DeleteConfirm`] belongs to the lightbox view, like the Solid component's signals:
//! it is made new for every opening of the lightbox and starts over when another
//! asset is selected. The deletion itself is the model's
//! ([`LightboxCommand::DeleteSelected`](super::LightboxCommand::DeleteSelected)), which
//! sends it through the core's use case and nothing else: a refusal (a scan holds the
//! scan lock, ADR 0022) changes nothing and is shown here with every exit usable
//! again.

use std::sync::atomic::{AtomicU64, Ordering};

use super::model::{DeletedAsset, DeletedSource};
use crate::{
    i18n::{I18n, Params},
    library::MutationError,
    search::text::js_trim,
};

/// Request IDs are unique in the process, so the confirmation of a reopened lightbox
/// never takes the answer to a deletion of the one before it.
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

fn next_request() -> u64 {
    NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
}

/// What an accepted deletion has to tell the user besides closing the lightbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeleteNotice {
    /// The source file was missing already; the stale entry was removed.
    SourceMissing,
    /// The entry is removed, the cleanup of the staged file failed.
    CleanupPending { recovery_path: String },
}

impl DeleteNotice {
    /// The notice of a deletion's answer; none for a plain deletion, a refusal or a
    /// command without a selection. It is shown whatever the lightbox shows by then.
    pub fn of(result: &Result<Option<DeletedAsset>, MutationError>) -> Option<Self> {
        let deleted = result.as_ref().ok()?.as_ref()?;
        match deleted.source {
            DeletedSource::Deleted => None,
            DeletedSource::Missing => Some(Self::SourceMissing),
            DeletedSource::CleanupPending => Some(Self::CleanupPending {
                recovery_path: deleted.recovery_path.clone().unwrap_or_default(),
            }),
        }
    }

    /// The text of the notice.
    pub fn text(&self, i18n: &I18n<'_>) -> String {
        match self {
            Self::SourceMissing => i18n.t("lightbox.deleteConfirm.sourceMissing"),
            Self::CleanupPending { recovery_path } => i18n.t_with(
                "lightbox.deleteConfirm.cleanupPending",
                &Params::new().with("path", recovery_path),
            ),
        }
    }
}

/// The confirmation's state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeleteConfirm {
    asset_id: Option<i64>,
    open: bool,
    text: String,
    /// The request of the deletion that runs.
    deleting: Option<u64>,
    /// The backend's text of the last refusal, as it came.
    error: Option<String>,
}

impl DeleteConfirm {
    pub fn new() -> Self {
        Self::default()
    }

    /// The selected asset. Another asset starts over: the confirmation closes, the
    /// typed word and the error go, and the answer of a deletion that still runs is
    /// no longer this confirmation's. A republished selection of the same asset (a
    /// tag or favorite write) changes nothing.
    pub fn sync(&mut self, selected: Option<i64>) {
        if selected == self.asset_id {
            return;
        }
        *self = Self {
            asset_id: selected,
            ..Self::default()
        };
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The deletion was sent and has not answered: Close, Escape, the backdrop,
    /// navigation and Cancel are off.
    pub fn deleting(&self) -> bool {
        self.deleting.is_some()
    }

    /// The side panel stays open and cannot be collapsed.
    pub fn locks_sidebar(&self) -> bool {
        self.open || self.deleting()
    }

    /// The typed text, as typed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The backend's text of the last refusal.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The alert of the last refusal: the metadata was preserved, and why.
    pub fn error_text(&self, i18n: &I18n<'_>) -> Option<String> {
        let error = self.error.as_deref()?;
        Some(i18n.t_with(
            "lightbox.deleteConfirm.failedMetadataPreserved",
            &Params::new().with("error", i18n.backend_text(error)),
        ))
    }

    /// The delete button of the action rail: opens the confirmation without an old
    /// error.
    pub fn open(&mut self) {
        self.open = true;
        self.error = None;
    }

    /// Cancel, or Escape while the confirmation is open: closes it and forgets the
    /// typed word. `false` while the deletion runs: nothing closes.
    pub fn close(&mut self) -> bool {
        if self.deleting() {
            return false;
        }
        self.open = false;
        self.text.clear();
        true
    }

    /// The field's text. The field is disabled while the deletion runs.
    pub fn set_text(&mut self, text: impl Into<String>) {
        if !self.deleting() {
            self.text = text.into();
        }
    }

    /// Whether the typed text is the confirmation `word` of the language: compared
    /// without the whitespace around it and without regard to letter case.
    pub fn can_confirm(&self, word: &str) -> bool {
        let word = fold(word);
        !word.is_empty() && fold(&self.text) == word
    }

    /// Confirm: the request to send as
    /// [`LightboxCommand::DeleteSelected`](super::LightboxCommand::DeleteSelected).
    /// `None` while the confirmation is closed, the word is not typed or a deletion
    /// runs.
    pub fn submit(&mut self, word: &str) -> Option<u64> {
        if !self.open || self.deleting() || !self.can_confirm(word) {
            return None;
        }
        let request = next_request();
        self.deleting = Some(request);
        self.error = None;
        Some(request)
    }

    /// The answer to the deletion of `request`. An accepted one closes the
    /// confirmation; a refusal keeps it open with the backend's text and unlocks the
    /// exits. An answer to a request that is not this confirmation's changes nothing.
    pub fn settled(&mut self, request: u64, result: &Result<Option<DeletedAsset>, MutationError>) {
        if self.deleting != Some(request) {
            return;
        }
        self.deleting = None;
        match result {
            Ok(_) => {
                self.open = false;
                self.text.clear();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

/// The comparison form of a confirmation word: trimmed, composed and lowercased
/// (`value.trim().normalize("NFC").toLocaleLowerCase(language)`). No shipped language
/// has locale-specific case rules, so the lowercase is the plain Unicode one.
fn fold(value: &str) -> String {
    compose(js_trim(value)).to_lowercase()
}

/// A precomposed vowel for a vowel followed by a combining grave or acute accent.
fn accented(base: char, mark: char) -> Option<char> {
    const GRAVE: char = '\u{300}';
    const ACUTE: char = '\u{301}';
    let table: &[(char, char, char)] = &[
        ('a', 'à', 'á'),
        ('e', 'è', 'é'),
        ('i', 'ì', 'í'),
        ('o', 'ò', 'ó'),
        ('u', 'ù', 'ú'),
        ('y', 'ỳ', 'ý'),
        ('A', 'À', 'Á'),
        ('E', 'È', 'É'),
        ('I', 'Ì', 'Í'),
        ('O', 'Ò', 'Ó'),
        ('U', 'Ù', 'Ú'),
        ('Y', 'Ỳ', 'Ý'),
    ];
    let (_, grave, acute) = table.iter().find(|(letter, _, _)| *letter == base)?;
    match mark {
        GRAVE => Some(*grave),
        ACUTE => Some(*acute),
        _ => None,
    }
}

const HANGUL_SYLLABLES: u32 = 0xAC00;
const HANGUL_LEADS: std::ops::Range<u32> = 0x1100..0x1113;
const HANGUL_VOWELS: std::ops::Range<u32> = 0x1161..0x1176;
/// The trailing consonants; U+11A7 itself is the "none" of the arithmetic.
const HANGUL_TRAILS: std::ops::Range<u32> = 0x11A8..0x11C3;
const HANGUL_VOWEL_COUNT: u32 = 21;
const HANGUL_TRAIL_COUNT: u32 = 28;

/// A Hangul syllable for a syllable start and the jamo that follows it (the
/// arithmetic of the Unicode standard, chapter 3.12).
fn hangul(previous: char, next: char) -> Option<char> {
    let (previous, next) = (previous as u32, next as u32);
    if HANGUL_LEADS.contains(&previous) && HANGUL_VOWELS.contains(&next) {
        let lead = previous - HANGUL_LEADS.start;
        let vowel = next - HANGUL_VOWELS.start;
        return char::from_u32(
            HANGUL_SYLLABLES + (lead * HANGUL_VOWEL_COUNT + vowel) * HANGUL_TRAIL_COUNT,
        );
    }
    let syllable = previous.checked_sub(HANGUL_SYLLABLES)?;
    let open = syllable < 19 * HANGUL_VOWEL_COUNT * HANGUL_TRAIL_COUNT
        && syllable % HANGUL_TRAIL_COUNT == 0;
    if open && HANGUL_TRAILS.contains(&next) {
        return char::from_u32(previous + (next - HANGUL_TRAILS.start + 1));
    }
    None
}

/// The composed spelling of what the confirmation words of the shipped languages
/// need: vowels with a grave or acute accent (`Sì`, `Sí`) and Hangul syllables (`예`).
/// This is that part of NFC, not all of it; a text decomposed in any other way is
/// compared as it is.
fn compose(text: &str) -> String {
    let mut composed = String::with_capacity(text.len());
    for next in text.chars() {
        let joined = composed
            .chars()
            .next_back()
            .and_then(|previous| accented(previous, next).or_else(|| hangul(previous, next)));
        match joined {
            Some(joined) => {
                composed.pop();
                composed.push(joined);
            }
            None => composed.push(next),
        }
    }
    composed
}

#[cfg(test)]
mod tests {
    use super::{compose, DeleteConfirm, DeleteNotice};
    use crate::{
        backend::BackendError,
        i18n::{I18n, Language},
        library::MutationError,
        lightbox::{DeletedAsset, DeletedSource},
    };

    const WORD_KEY: &str = "lightbox.deleteConfirm.confirmWord";

    fn deleted(source: DeletedSource, recovery_path: Option<&str>) -> DeletedAsset {
        DeletedAsset {
            source,
            recovery_path: recovery_path.map(str::to_owned),
            removed_assets: 1,
            removed_thumbnails: 0,
        }
    }

    fn open_with(text: &str) -> DeleteConfirm {
        let mut confirm = DeleteConfirm::new();
        confirm.sync(Some(1));
        confirm.open();
        confirm.set_text(text);
        confirm
    }

    /// The decomposed spelling of a confirmation word: accents and Hangul jamo apart.
    fn decompose(text: &str) -> String {
        let mut decomposed = String::new();
        for c in text.chars() {
            match c {
                'ì' => decomposed.push_str("i\u{300}"),
                'í' => decomposed.push_str("i\u{301}"),
                '\u{AC00}'..='\u{D7A3}' => {
                    let index = c as u32 - 0xAC00;
                    let (lead, vowel, trail) = (index / 588, index % 588 / 28, index % 28);
                    decomposed.extend(char::from_u32(0x1100 + lead));
                    decomposed.extend(char::from_u32(0x1161 + vowel));
                    if trail > 0 {
                        decomposed.extend(char::from_u32(0x11A7 + trail));
                    }
                }
                other => decomposed.push(other),
            }
        }
        decomposed
    }

    #[test]
    fn the_word_is_compared_trimmed_and_without_case_in_every_language() {
        for language in Language::ALL {
            let word = I18n::new(language).t(WORD_KEY);
            assert_ne!(word, WORD_KEY, "{language:?} has a confirmation word");
            for typed in [
                word.clone(),
                format!("  {} ", word.to_uppercase()),
                word.to_lowercase(),
                decompose(&word),
            ] {
                assert!(
                    open_with(&typed).can_confirm(&word),
                    "{language:?}: {typed:?} confirms {word:?}"
                );
            }
            assert!(!open_with("").can_confirm(&word));
            assert!(!open_with(&format!("{word}!")).can_confirm(&word));
        }
        assert!(
            !open_with("").can_confirm("  "),
            "an empty word confirms nothing"
        );
    }

    #[test]
    fn composition_covers_accents_and_hangul_only() {
        assert_eq!(compose("Si\u{300}"), "Sì");
        assert_eq!(compose("SI\u{301}"), "SÍ");
        assert_eq!(compose("\u{110B}\u{1168}"), "예");
        assert_eq!(compose("\u{1112}\u{1161}\u{11AB}"), "한");
        assert_eq!(compose("n\u{303}"), "n\u{303}", "left as typed");
    }

    #[test]
    fn a_deletion_locks_the_confirmation_until_it_answers() {
        let mut confirm = open_with("yes");
        assert!(confirm.locks_sidebar());
        let request = confirm.submit("Yes").expect("the word is typed");
        assert!(confirm.deleting());
        assert_eq!(confirm.submit("Yes"), None, "one deletion at a time");
        assert!(!confirm.close(), "no exit while the deletion runs");
        confirm.set_text("changed");
        assert_eq!(confirm.text(), "yes");

        confirm.settled(request, &Ok(Some(deleted(DeletedSource::Deleted, None))));
        assert!(!confirm.deleting());
        assert!(!confirm.is_open());
        assert_eq!(confirm.text(), "");
    }

    #[test]
    fn a_refusal_keeps_the_confirmation_open_with_the_translated_reason() {
        let mut confirm = open_with("yes");
        let request = confirm.submit("Yes").unwrap();
        let refused = Err(MutationError::Backend(BackendError::rejected(
            "i18n:common.libraryBusy",
        )));

        // An answer to another request is not this confirmation's.
        confirm.settled(request + 1, &refused);
        assert!(confirm.deleting());

        confirm.settled(request, &refused);
        assert!(!confirm.deleting());
        assert!(confirm.is_open());
        assert_eq!(confirm.error(), Some("i18n:common.libraryBusy"));
        let i18n = I18n::new(Language::En);
        let text = confirm.error_text(&i18n).unwrap();
        assert!(text.contains(&i18n.t("common.libraryBusy")), "{text}");
        assert!(!text.contains("i18n:"), "{text}");
        assert!(confirm.close(), "every exit is usable again");

        confirm.open();
        assert_eq!(confirm.error(), None, "reopening forgets the refusal");
    }

    #[test]
    fn another_asset_starts_over_and_the_same_asset_keeps_the_typed_word() {
        let mut confirm = open_with("ye");
        confirm.sync(Some(1));
        assert!(confirm.is_open());
        assert_eq!(confirm.text(), "ye");

        let request = {
            confirm.set_text("yes");
            confirm.submit("yes").unwrap()
        };
        confirm.sync(Some(2));
        assert!(!confirm.is_open());
        assert!(!confirm.deleting());
        assert_eq!(confirm.text(), "");
        confirm.open();
        confirm.settled(request, &Ok(Some(deleted(DeletedSource::Deleted, None))));
        assert!(confirm.is_open(), "the old answer closes nothing");
    }

    #[test]
    fn a_missing_source_and_a_pending_cleanup_are_announced() {
        let i18n = I18n::new(Language::En);
        assert_eq!(
            DeleteNotice::of(&Ok(Some(deleted(DeletedSource::Deleted, None)))),
            None
        );
        assert_eq!(DeleteNotice::of(&Ok(None)), None);
        assert_eq!(
            DeleteNotice::of(&Ok(Some(deleted(DeletedSource::Missing, None)))),
            Some(DeleteNotice::SourceMissing)
        );
        let pending = DeleteNotice::of(&Ok(Some(deleted(
            DeletedSource::CleanupPending,
            Some("/synthetic/.mediatagger-1.pending"),
        ))))
        .unwrap();
        assert!(pending
            .text(&i18n)
            .contains("/synthetic/.mediatagger-1.pending"));
        assert_eq!(
            DeleteNotice::SourceMissing.text(&i18n),
            i18n.t("lightbox.deleteConfirm.sourceMissing")
        );
    }
}
