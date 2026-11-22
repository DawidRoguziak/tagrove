//! One suggesting text field (`SearchTagator.tsx` with `createSuggestions.ts`), as a
//! state machine the view feeds with focus, typing, caret moves and keys.
//!
//! # The session
//!
//! The list opens when the focused field's trimmed value is not empty (on focus, a
//! click, typing) and closes on Escape, blur, disabling and an empty value. While open
//! and enabled the field asks the [`SuggestionClient`](super::SuggestionClient) for
//! the token under the caret, unless it has none or it is an unquoted positive
//! operator (`tags`, `gn:…`); a changed request replaces the earlier one. Results
//! are shown only for the current request and vocabulary; while they are pending,
//! nothing is shown ([`Combobox::items`] is empty).
//!
//! # Keys
//!
//! With items shown, Up and Down wrap through them, Enter picks the active one and
//! Escape closes the list (consumed). Otherwise Enter submits and Escape is not
//! handled, so it reaches the next layer. The search field auto-selects the first
//! item; the tag editors do not ([`ComboboxConfig`]).
//!
//! # Failure
//!
//! A failed request leaves a Retry ([`Combobox::failed`]) for as long as the context
//! (token, used and excluded tags) and the vocabulary are the same, also across blur;
//! Retry focuses the field, opens the session and searches again.
//!
//! Offsets are UTF-8 byte offsets of the field's text.

use std::sync::Arc;

use super::{
    active_token::{
        apply_suggestion_to_value, collect_used_tags, read_active_token, suggestion_replacement,
    },
    matcher::{is_meta_filter_query, TagSuggestion},
    suggestions::{
        EditorId, SuggestionContext, SuggestionEvent, SuggestionFailure, SuggestionInput,
        SuggestionRequest,
    },
    text::js_trim,
};

/// How a field uses its suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComboboxConfig {
    /// The first item is active when items arrive (`autoSelectFirstSuggestion`).
    pub auto_select_first: bool,
    /// A pick is handed to the owner ([`ComboboxEffect::Pick`], `onSuggestionPick`)
    /// instead of replacing the token.
    pub pick_to_owner: bool,
    /// With [`Self::pick_to_owner`]: the list stays open after a pick
    /// (`keepSuggestionsOpenOnPick`).
    pub keep_open_on_pick: bool,
}

impl ComboboxConfig {
    /// The search field: picks replace the token, the first item is active.
    pub const SEARCH: Self = Self {
        auto_select_first: true,
        pick_to_owner: false,
        keep_open_on_pick: false,
    };
}

impl Default for ComboboxConfig {
    fn default() -> Self {
        Self::SEARCH
    }
}

/// The keys of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboboxKey {
    Up,
    Down,
    Enter,
    Escape,
}

/// What the owner of the field does.
#[derive(Debug)]
pub enum ComboboxEffect {
    /// For the suggestion client.
    Suggest(SuggestionInput),
    /// Put `text` in place of `range` of the field (one undo step) and the caret at
    /// `caret`; the field's value is then `value`. The combobox already knows it.
    ReplaceToken {
        range: std::ops::Range<usize>,
        text: String,
        value: String,
        caret: usize,
    },
    /// A picked tag for the owner (`onSuggestionPick`).
    Pick(String),
    /// Enter without an active item (`onSubmit`).
    Submit,
    /// Give the field the focus.
    FocusInput,
}

/// The result shown for a request.
#[derive(Debug, Clone)]
struct Answer {
    request: SuggestionRequest,
    vocabulary: Arc<Vec<String>>,
    items: Arc<[TagSuggestion]>,
}

#[derive(Debug, Clone)]
pub struct Combobox {
    editor: EditorId,
    config: ComboboxConfig,
    value: String,
    caret: usize,
    focused: bool,
    open: bool,
    disabled: bool,
    active: Option<usize>,
    vocabulary: Arc<Vec<String>>,
    excluded: Vec<String>,
    retry: u64,
    /// The request on its way, with its vocabulary.
    sent: Option<(SuggestionRequest, Arc<Vec<String>>)>,
    answer: Option<Answer>,
    failure: Option<(SuggestionContext, Arc<Vec<String>>)>,
    /// The items shown after the last input.
    shown: Arc<[TagSuggestion]>,
}

fn empty_items() -> Arc<[TagSuggestion]> {
    Arc::from(Vec::new())
}

impl Combobox {
    pub fn new(editor: EditorId, config: ComboboxConfig, vocabulary: Arc<Vec<String>>) -> Self {
        Self {
            editor,
            config,
            value: String::new(),
            caret: 0,
            focused: false,
            open: false,
            disabled: false,
            active: if config.auto_select_first {
                Some(0)
            } else {
                None
            },
            vocabulary,
            excluded: Vec::new(),
            retry: 0,
            sent: None,
            answer: None,
            failure: None,
            shown: empty_items(),
        }
    }

    pub fn editor(&self) -> EditorId {
        self.editor
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// The active item; `None` when none is (the tag editors before an arrow key).
    pub fn active(&self) -> Option<usize> {
        self.active.filter(|index| *index < self.shown.len())
    }

    /// The items shown: the answer to the current request, empty while it is pending
    /// or when the session is closed.
    pub fn items(&self) -> &Arc<[TagSuggestion]> {
        &self.shown
    }

    /// `sessionOpen`: the list is mounted (it is drawn only with items).
    pub fn session_open(&self) -> bool {
        self.focused && self.open && !js_trim(&self.value).is_empty() && !self.disabled
    }

    /// `popup`: the list is shown (`aria-expanded`).
    pub fn popup(&self) -> bool {
        self.session_open() && !self.shown.is_empty()
    }

    /// The Retry button is shown.
    pub fn failed(&self) -> bool {
        !self.disabled
            && self.failure.as_ref().is_some_and(|(context, vocabulary)| {
                *context == self.context() && Arc::ptr_eq(vocabulary, &self.vocabulary)
            })
    }

    fn default_active(&self) -> Option<usize> {
        self.config.auto_select_first.then_some(0)
    }

    fn eligible(&self) -> bool {
        self.focused && self.open && !self.disabled
    }

    fn context(&self) -> SuggestionContext {
        SuggestionContext {
            token: read_active_token(&self.value, self.caret),
            used: collect_used_tags(&self.value, &self.excluded),
            excluded: self.excluded.clone(),
        }
    }

    /// The request the field makes now, if any.
    fn wanted(&self) -> Option<SuggestionRequest> {
        if !self.eligible() {
            return None;
        }
        let context = self.context();
        let token = context.token.as_ref()?;
        let query = js_trim(&token.query);
        if query.is_empty() || (!token.literal && !token.negative && is_meta_filter_query(query)) {
            return None;
        }
        Some(SuggestionRequest {
            context,
            retry: self.retry,
        })
    }

    /// Brings everything derived in line after an input: closes a session that may not
    /// stay open, sends or cancels the request, and moves the active item with the
    /// items shown.
    fn sync(&mut self, effects: &mut Vec<ComboboxEffect>) {
        if !self.focused || js_trim(&self.value).is_empty() || self.disabled {
            self.open = false;
        }
        let wanted = self.wanted();
        match &wanted {
            Some(request) => {
                let current = self.sent.as_ref().is_some_and(|(sent, vocabulary)| {
                    sent == request && Arc::ptr_eq(vocabulary, &self.vocabulary)
                });
                if !current {
                    self.failure = None;
                    self.sent = Some((request.clone(), Arc::clone(&self.vocabulary)));
                    effects.push(ComboboxEffect::Suggest(SuggestionInput::Search {
                        editor: self.editor,
                        vocabulary: Arc::clone(&self.vocabulary),
                        request: request.clone(),
                    }));
                }
            }
            None => {
                if self.sent.take().is_some() {
                    effects.push(ComboboxEffect::Suggest(SuggestionInput::Cancel {
                        editor: self.editor,
                    }));
                }
            }
        }
        let items = match (&wanted, &self.answer) {
            (Some(request), Some(answer))
                if answer.request == *request
                    && Arc::ptr_eq(&answer.vocabulary, &self.vocabulary) =>
            {
                Arc::clone(&answer.items)
            }
            _ => empty_items(),
        };
        if !Arc::ptr_eq(&items, &self.shown) && !(items.is_empty() && self.shown.is_empty()) {
            let previous = self
                .active
                .and_then(|index| self.shown.get(index))
                .map(|item| item.value.clone());
            let kept = previous.and_then(|value| items.iter().position(|item| item.value == value));
            self.active = if items.is_empty() {
                self.default_active()
            } else if kept.is_some() {
                kept
            } else if self.active.is_none() {
                self.default_active()
            } else {
                self.active.map(|index| index.min(items.len() - 1))
            };
            self.shown = items;
        }
    }

    fn done(&mut self, mut effects: Vec<ComboboxEffect>) -> Vec<ComboboxEffect> {
        self.sync(&mut effects);
        effects
    }

    /// The field got the focus (`focusin`), with the caret at `caret`.
    pub fn focus_in(&mut self, caret: usize) -> Vec<ComboboxEffect> {
        self.focused = true;
        self.caret = caret;
        self.open = !js_trim(&self.value).is_empty();
        self.done(Vec::new())
    }

    /// The field lost the focus (`focusout`).
    pub fn focus_out(&mut self) -> Vec<ComboboxEffect> {
        self.focused = false;
        self.open = false;
        self.done(Vec::new())
    }

    /// The user changed the text (`input`).
    pub fn input(&mut self, value: &str, caret: usize) -> Vec<ComboboxEffect> {
        self.value = value.to_owned();
        self.caret = caret;
        self.open = !js_trim(value).is_empty();
        self.active = self.default_active();
        self.done(Vec::new())
    }

    /// The caret or the selection moved (`select`).
    pub fn caret_moved(&mut self, caret: usize) -> Vec<ComboboxEffect> {
        self.caret = caret;
        self.done(Vec::new())
    }

    /// A click in the field.
    pub fn click(&mut self, caret: usize) -> Vec<ComboboxEffect> {
        self.caret = caret;
        if !self.disabled {
            self.open = !js_trim(&self.value).is_empty();
        }
        self.done(Vec::new())
    }

    /// The owner changed the value (the `value` prop: a clear, an applied tag list);
    /// the session is not opened by it.
    pub fn set_value(&mut self, value: &str, caret: usize) -> Vec<ComboboxEffect> {
        self.value = value.to_owned();
        self.caret = caret.min(value.len());
        self.done(Vec::new())
    }

    /// The known tags (`knownTags`); the same `Arc` changes nothing.
    pub fn set_vocabulary(&mut self, vocabulary: &Arc<Vec<String>>) -> Vec<ComboboxEffect> {
        if Arc::ptr_eq(vocabulary, &self.vocabulary) {
            return Vec::new();
        }
        self.vocabulary = Arc::clone(vocabulary);
        self.done(Vec::new())
    }

    /// Tags never to suggest (`excludedTags`).
    pub fn set_excluded(&mut self, excluded: Vec<String>) -> Vec<ComboboxEffect> {
        if excluded == self.excluded {
            return Vec::new();
        }
        self.excluded = excluded;
        self.done(Vec::new())
    }

    pub fn set_disabled(&mut self, disabled: bool) -> Vec<ComboboxEffect> {
        self.disabled = disabled;
        self.done(Vec::new())
    }

    /// A key; `true` when the combobox took it, otherwise it goes on (the field moves
    /// the caret, a layer takes Escape).
    pub fn key(&mut self, key: ComboboxKey) -> (bool, Vec<ComboboxEffect>) {
        if self.disabled {
            return (false, Vec::new());
        }
        let count = self.shown.len();
        if self.open && count > 0 {
            match key {
                ComboboxKey::Up | ComboboxKey::Down => {
                    let down = key == ComboboxKey::Down;
                    self.active = Some(match self.active {
                        None => {
                            if down {
                                0
                            } else {
                                count - 1
                            }
                        }
                        Some(index) => {
                            let index = index.min(count - 1);
                            if down {
                                (index + 1) % count
                            } else {
                                (index + count - 1) % count
                            }
                        }
                    });
                    return (true, Vec::new());
                }
                ComboboxKey::Enter => {
                    if let Some(index) = self.active.filter(|index| *index < count) {
                        let value = self.shown[index].value.clone();
                        return (true, self.pick(&value, index));
                    }
                }
                ComboboxKey::Escape => {
                    self.open = false;
                    return (true, self.done(Vec::new()));
                }
            }
        }
        if key == ComboboxKey::Enter {
            return (true, vec![ComboboxEffect::Submit]);
        }
        (false, Vec::new())
    }

    /// A press on the item at `index` (`onMouseDown` of an option).
    pub fn pick_at(&mut self, index: usize) -> Vec<ComboboxEffect> {
        match self.shown.get(index) {
            Some(item) => {
                let value = item.value.clone();
                self.pick(&value, index)
            }
            None => Vec::new(),
        }
    }

    fn pick(&mut self, tag: &str, index: usize) -> Vec<ComboboxEffect> {
        let mut effects = Vec::new();
        if self.config.pick_to_owner {
            effects.push(ComboboxEffect::Pick(tag.to_owned()));
            let keep =
                self.config.keep_open_on_pick && self.focused && !js_trim(&self.value).is_empty();
            self.open = keep;
            self.active = if keep {
                Some(index)
            } else {
                self.default_active()
            };
            if self.config.keep_open_on_pick {
                effects.push(ComboboxEffect::FocusInput);
            }
            return self.done(effects);
        }
        let Some(token) = read_active_token(&self.value, self.caret) else {
            return Vec::new();
        };
        let (value, caret) = apply_suggestion_to_value(&self.value, &token, tag);
        effects.push(ComboboxEffect::ReplaceToken {
            range: token.start..token.end,
            text: suggestion_replacement(&token, tag),
            value: value.clone(),
            caret,
        });
        self.value = value;
        self.caret = caret;
        self.open = false;
        self.active = self.default_active();
        self.done(effects)
    }

    /// The Retry button: focus, open the session and search again.
    pub fn retry(&mut self) -> Vec<ComboboxEffect> {
        self.open = !js_trim(&self.value).is_empty();
        self.retry += 1;
        self.done(vec![
            ComboboxEffect::FocusInput,
            ComboboxEffect::Suggest(SuggestionInput::Retry),
        ])
    }

    /// An answer of the suggestion client; one for another field or an older request
    /// changes nothing.
    pub fn receive(&mut self, event: SuggestionEvent) -> Vec<ComboboxEffect> {
        let SuggestionEvent::Settled {
            editor,
            request,
            vocabulary,
            result,
        } = event;
        let current = editor == self.editor
            && self.sent.as_ref().is_some_and(|(sent, sent_vocabulary)| {
                *sent == request && Arc::ptr_eq(sent_vocabulary, &vocabulary)
            });
        if !current {
            return Vec::new();
        }
        match result {
            Ok(items) => {
                self.answer = Some(Answer {
                    request,
                    vocabulary,
                    items,
                });
            }
            Err(SuggestionFailure) => {
                self.failure = Some((request.context, vocabulary));
            }
        }
        self.done(Vec::new())
    }

    /// The field goes away: its request is cancelled.
    pub fn dispose(&mut self) -> Vec<ComboboxEffect> {
        self.sent = None;
        vec![ComboboxEffect::Suggest(SuggestionInput::Cancel {
            editor: self.editor,
        })]
    }
}

#[cfg(test)]
mod tests;
