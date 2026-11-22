//! The tag list (`src/features/search/tag-list`): every tag of the library in pages,
//! filtered by a query, from which the user picks tags to include and to exclude and
//! applies them as a new search.
//!
//! [`TagList`] is `createTagList` together with the state `TagListModal` keeps: whether
//! the list is open, the filter, the selections, the timer that tells a single click
//! from a double click and whether Apply runs. It is a model in the convention of
//! [`crate::model`], embedded in the application runtime
//! ([`crate::runtime::RuntimeInput::TagList`]).
//!
//! # Pages
//!
//! Opening the list, a new trimmed filter or a new vocabulary while open starts over:
//! the known tags matching the filter are shown at once (the fallback, prepared only
//! while open), the scroll position goes back to the top
//! ([`TagList::scroll_resets`]) and the first page of [`TAG_LIST_PAGE_SIZE`] is asked
//! for. Its answer replaces the fallback; a failure keeps it. One page request runs at
//! a time; a request that was started over, or a list that was closed, drops its
//! answer.
//!
//! The next page is loaded when the list is scrolled within
//! [`LOAD_MORE_THRESHOLD`] of its end, and on its own while the content is no more
//! than that taller than the list ([`ListMetrics`], reported by the view after layout).
//! A next page that fails is replaced by the fallback's tags at that offset; when that
//! adds nothing, loading on its own stops until the next start, and only a scroll asks
//! again.
//!
//! # Choices
//!
//! A click waits [`SINGLE_CLICK_DELAY`] and then includes an inactive tag or makes a
//! selected one inactive; a click on another tag in the meantime applies the waiting one
//! first. A double click (or Shift+Enter, Shift+Space) cancels the waiting click and
//! excludes an inactive or included tag, or includes an excluded one. Apply builds the
//! filter text (`buildFilterInput`) and asks the runtime to apply it
//! ([`TagListEffect::ApplySearch`]); the runtime answers with
//! [`TagListInput::ApplySettled`] once the query it started has answered. Success
//! closes the list; failure keeps it open with its choices for another try. While
//! Apply runs, the choices and closing are refused.
//!
//! Closing forgets the filter, the choices and the pages.

pub mod layout;
pub mod rows;
pub mod selection;

use std::{sync::Arc, time::Duration};

use media_tagger_core::models::TagListPage;

use crate::{
    backend::BackendResult,
    model::{Model, RequestToken, Tokens},
    search::text::js_trim,
};

pub use selection::{
    compare_tags, create_fallback_page, dedupe_known_tags, merge_tag_pages, normalize_tag_name,
    sort_tags_case_insensitive, TagPage, TagSelectionMode, TagSelections,
};

/// Tags per `list_tags` call (`PAGE_SIZE` of `createTagList`).
pub const TAG_LIST_PAGE_SIZE: usize = 100;
/// How long a click waits for a second one.
pub const SINGLE_CLICK_DELAY: Duration = Duration::from_millis(220);
/// How close to its end, in pixels, the list loads its next page.
pub const LOAD_MORE_THRESHOLD: f32 = 120.0;

/// The scroll container of the list as last laid out, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ListMetrics {
    /// The height of the content (`scrollHeight`).
    pub content_height: f32,
    /// The height of the visible part (`clientHeight`).
    pub viewport_height: f32,
    /// How far the content is scrolled (`scrollTop`).
    pub scroll_top: f32,
}

impl ListMetrics {
    /// The end of the content is within the threshold (the scroll handler's test).
    pub fn near_end(&self) -> bool {
        self.content_height - self.scroll_top - self.viewport_height <= LOAD_MORE_THRESHOLD
    }

    /// The content is hardly taller than the list (the automatic test).
    pub fn fits(&self) -> bool {
        self.content_height <= self.viewport_height + LOAD_MORE_THRESHOLD
    }

    /// Whether the view reports this measurement, given the one the model holds. A
    /// new size always counts; a scroll counts only near the end, where it may load the
    /// next page, and on the frame it leaves that zone (so that coming back is a
    /// scroll again). Scrolling in the middle of a long list reports nothing, so it
    /// costs no round trip through the runtime and no second render.
    pub fn worth_reporting(&self, known: Option<ListMetrics>) -> bool {
        let Some(known) = known else {
            return true;
        };
        if known == *self {
            return false;
        }
        known.content_height != self.content_height
            || known.viewport_height != self.viewport_height
            || self.near_end()
            || known.near_end()
    }
}

/// Commands, backend replies and timers.
#[derive(Debug)]
pub enum TagListInput {
    /// The launcher was pressed.
    Open,
    /// Cancel, the close button, Escape or a press on the backdrop; refused while Apply
    /// runs. Also sent when the view goes away.
    Close,
    /// The known tags of the library; the same `Arc` changes nothing.
    SetVocabulary(Arc<Vec<String>>),
    /// The filter field's text.
    SetQuery(String),
    /// The list was laid out or scrolled.
    ListMetrics(ListMetrics),
    /// Load the next page now, whatever stopped automatic loading.
    LoadMore,
    /// A click on a tag; `count` is the click count of the press (a keyboard
    /// activation counts 1). Only the first click of a series counts.
    Click {
        tag: String,
        count: usize,
    },
    /// A double click, Shift+Enter or Shift+Space on a tag.
    DoubleClick {
        tag: String,
    },
    /// The single-click timer fired.
    ClickTimerFired(RequestToken),
    Apply,
    /// The runtime applied the search: `ok` unless the query it started failed.
    ApplySettled {
        ok: bool,
    },
    PageLoaded {
        token: RequestToken,
        result: BackendResult<TagListPage>,
    },
}

/// Work for the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagListEffect {
    /// `list_tags`; answered with [`TagListInput::PageLoaded`].
    LoadPage {
        token: RequestToken,
        query: String,
        offset: i64,
        limit: i64,
    },
    /// Send [`TagListInput::ClickTimerFired`] after `delay`.
    ClickTimer {
        token: RequestToken,
        delay: Duration,
    },
    /// Replace the search field's text with this and submit it; answer with
    /// [`TagListInput::ApplySettled`].
    ApplySearch(String),
}

#[derive(Debug, Clone)]
enum PageRequest {
    First {
        token: RequestToken,
        fallback: TagPage,
    },
    Next {
        token: RequestToken,
        offset: usize,
        fallback: TagPage,
    },
}

impl PageRequest {
    fn token(&self) -> RequestToken {
        match self {
            Self::First { token, .. } | Self::Next { token, .. } => *token,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TagList {
    open: bool,
    query: String,
    vocabulary: Arc<Vec<String>>,
    /// The deduplicated known tags while open (`fallback`), empty while closed.
    fallback: Vec<String>,
    fallback_builds: u64,
    page: TagPage,
    page_version: u64,
    loading: bool,
    loading_more: bool,
    auto_load_blocked: bool,
    request: Option<PageRequest>,
    tokens: Tokens,
    metrics: Option<ListMetrics>,
    scroll_top: f32,
    scroll_resets: u64,
    selections: TagSelections,
    pending_click: Option<(String, RequestToken)>,
    applying: bool,
}

impl Default for TagList {
    fn default() -> Self {
        Self::new()
    }
}

impl TagList {
    pub fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            vocabulary: Arc::new(Vec::new()),
            fallback: Vec::new(),
            fallback_builds: 0,
            page: TagPage::default(),
            page_version: 0,
            loading: false,
            loading_more: false,
            auto_load_blocked: false,
            request: None,
            tokens: Tokens::default(),
            metrics: None,
            scroll_top: 0.0,
            scroll_resets: 0,
            selections: TagSelections::new(),
            pending_click: None,
            applying: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The filter as typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The tags shown and how many match.
    pub fn page(&self) -> &TagPage {
        &self.page
    }

    /// Counts the replacements of [`page`](Self::page).
    pub fn page_version(&self) -> u64 {
        self.page_version
    }

    /// The first page is on its way (`loading`).
    pub fn loading(&self) -> bool {
        self.loading
    }

    /// A next page is on its way (`loadingMore`).
    pub fn loading_more(&self) -> bool {
        self.loading_more
    }

    /// A failed next page added nothing: no more loading on its own until the next
    /// start.
    pub fn auto_load_blocked(&self) -> bool {
        self.auto_load_blocked
    }

    pub fn selections(&self) -> &TagSelections {
        &self.selections
    }

    /// A click waits for its timer.
    pub fn click_pending(&self) -> bool {
        self.pending_click.is_some()
    }

    /// Apply runs: the choices, the filter and closing are disabled.
    pub fn applying(&self) -> bool {
        self.applying
    }

    /// The measurement of the list since the page last changed; `None` asks the view
    /// for a new one.
    pub fn metrics(&self) -> Option<ListMetrics> {
        self.metrics
    }

    /// Counts the starts that scroll the list back to its top.
    pub fn scroll_resets(&self) -> u64 {
        self.scroll_resets
    }

    /// How often the known tags were prepared for the fallback: only while open.
    pub fn fallback_builds(&self) -> u64 {
        self.fallback_builds
    }

    /// Which message the list shows instead of tags, if any: loading, no match or no
    /// tag at all (`TagListModal`'s list).
    pub fn placeholder_key(&self) -> Option<&'static str> {
        if !self.page.items.is_empty() {
            None
        } else if self.loading {
            Some("tagList.loading")
        } else if js_trim(&self.query).is_empty() {
            Some("tagList.empty")
        } else {
            Some("tagList.noMatches")
        }
    }

    fn publish(&mut self, page: TagPage) {
        self.page = page;
        self.page_version += 1;
        // Measured with the old content: the view measures again.
        self.metrics = None;
    }

    fn trimmed_query(&self) -> String {
        js_trim(&self.query).to_owned()
    }

    /// The effect of `createTagList` for a new open state, filter or vocabulary.
    fn start(&mut self, effects: &mut Vec<TagListEffect>) {
        self.auto_load_blocked = false;
        self.loading = self.open;
        self.loading_more = false;
        self.request = None;
        let query = self.trimmed_query();
        let fallback = create_fallback_page(&self.fallback, &query, 0, TAG_LIST_PAGE_SIZE);
        self.publish(fallback.clone());
        if !self.open {
            return;
        }
        self.scroll_resets += 1;
        self.scroll_top = 0.0;
        let token = self.tokens.issue();
        self.request = Some(PageRequest::First { token, fallback });
        effects.push(TagListEffect::LoadPage {
            token,
            query,
            offset: 0,
            limit: TAG_LIST_PAGE_SIZE as i64,
        });
    }

    fn prepare_fallback(&mut self) {
        self.fallback = if self.open {
            self.fallback_builds += 1;
            dedupe_known_tags(&self.vocabulary)
        } else {
            Vec::new()
        };
    }

    /// `loadMore`.
    fn load_more(&mut self, effects: &mut Vec<TagListEffect>) {
        if !self.open || self.request.is_some() || self.page.items.len() >= self.page.total {
            return;
        }
        self.loading_more = true;
        let offset = self.page.items.len();
        let query = self.trimmed_query();
        let fallback = create_fallback_page(&self.fallback, &query, offset, TAG_LIST_PAGE_SIZE);
        let token = self.tokens.issue();
        self.request = Some(PageRequest::Next {
            token,
            offset,
            fallback,
        });
        effects.push(TagListEffect::LoadPage {
            token,
            query,
            offset: i64::try_from(offset).unwrap_or(i64::MAX),
            limit: TAG_LIST_PAGE_SIZE as i64,
        });
    }

    /// The automatic pagination of `createTagList`, checked after every input.
    fn auto_load(&mut self, effects: &mut Vec<TagListEffect>) {
        let wanted = self.open
            && !self.loading
            && !self.loading_more
            && !self.auto_load_blocked
            && !self.page.items.is_empty()
            && self.page.items.len() < self.page.total
            && self.metrics.is_some_and(|metrics| metrics.fits());
        if wanted {
            self.load_more(effects);
        }
    }

    fn page_loaded(&mut self, token: RequestToken, result: BackendResult<TagListPage>) {
        let Some(request) = self.request.take_if(|request| request.token() == token) else {
            return;
        };
        match request {
            PageRequest::First { fallback, .. } => {
                let page = match result {
                    Ok(page) => TagPage {
                        items: page.items,
                        total: usize::try_from(page.total).unwrap_or(0),
                    },
                    Err(_) => fallback,
                };
                self.publish(page);
                self.loading = false;
            }
            PageRequest::Next {
                offset, fallback, ..
            } => {
                let page = match result {
                    Ok(next) => TagPage {
                        items: merge_tag_pages(&self.page.items, &next.items),
                        total: usize::try_from(next.total).unwrap_or(0),
                    },
                    Err(_) => TagPage {
                        items: merge_tag_pages(&self.page.items, &fallback.items),
                        total: self.page.total.max(fallback.total),
                    },
                };
                self.publish(page);
                self.auto_load_blocked = self.page.items.len() <= offset;
                self.loading_more = false;
            }
        }
    }

    fn cancel_click(&mut self) {
        self.pending_click = None;
    }

    /// Applies the waiting single click now.
    fn flush_click(&mut self) {
        if let Some((tag, _)) = self.pending_click.take() {
            self.selections = self.selections.apply_single(&tag);
        }
    }

    fn close(&mut self, effects: &mut Vec<TagListEffect>) {
        self.open = false;
        self.cancel_click();
        self.query.clear();
        self.selections = TagSelections::new();
        self.metrics = None;
        self.prepare_fallback();
        self.start(effects);
    }
}

impl Model for TagList {
    type Input = TagListInput;
    type Effect = TagListEffect;

    fn handle(&mut self, input: TagListInput) -> Vec<TagListEffect> {
        let mut effects = Vec::new();
        match input {
            TagListInput::Open => {
                if !self.open {
                    self.open = true;
                    self.prepare_fallback();
                    self.start(&mut effects);
                }
            }
            TagListInput::Close => {
                if self.open && !self.applying {
                    self.close(&mut effects);
                }
            }
            TagListInput::SetVocabulary(vocabulary) => {
                if !Arc::ptr_eq(&vocabulary, &self.vocabulary) {
                    self.vocabulary = vocabulary;
                    if self.open {
                        self.prepare_fallback();
                        self.start(&mut effects);
                    }
                }
            }
            TagListInput::SetQuery(query) => {
                if self.applying {
                    return effects;
                }
                let restart = js_trim(&query) != js_trim(&self.query);
                self.query = query;
                if restart && self.open {
                    self.start(&mut effects);
                }
            }
            TagListInput::ListMetrics(metrics) => {
                let scrolled = metrics.scroll_top != self.scroll_top;
                self.scroll_top = metrics.scroll_top;
                self.metrics = Some(metrics);
                if scrolled && metrics.near_end() {
                    self.load_more(&mut effects);
                }
            }
            TagListInput::LoadMore => self.load_more(&mut effects),
            TagListInput::Click { tag, count } => {
                if count > 1 || self.applying || !self.open {
                    return effects;
                }
                if self
                    .pending_click
                    .as_ref()
                    .is_some_and(|(pending, _)| *pending != tag)
                {
                    self.flush_click();
                }
                let token = self.tokens.issue();
                self.pending_click = Some((tag, token));
                effects.push(TagListEffect::ClickTimer {
                    token,
                    delay: SINGLE_CLICK_DELAY,
                });
            }
            TagListInput::DoubleClick { tag } => {
                if self.applying || !self.open {
                    return effects;
                }
                self.cancel_click();
                self.selections = self.selections.apply_double(&tag);
            }
            TagListInput::ClickTimerFired(token) => {
                if self
                    .pending_click
                    .as_ref()
                    .is_some_and(|(_, pending)| *pending == token)
                {
                    self.flush_click();
                }
            }
            TagListInput::Apply => {
                if self.applying || !self.open {
                    return effects;
                }
                self.flush_click();
                self.applying = true;
                effects.push(TagListEffect::ApplySearch(self.selections.filter_input()));
            }
            TagListInput::ApplySettled { ok } => {
                if !self.applying {
                    return effects;
                }
                self.applying = false;
                if ok && self.open {
                    self.close(&mut effects);
                }
            }
            TagListInput::PageLoaded { token, result } => self.page_loaded(token, result),
        }
        self.auto_load(&mut effects);
        effects
    }
}

#[cfg(test)]
mod tests;
