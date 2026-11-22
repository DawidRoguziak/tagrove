//! Fuzzy matching of tag suggestions (plan decision D13): what the Fuse.js worker of
//! `suggestions/suggestionService.ts` does, in Rust, run off the UI thread by the
//! suggestion client.
//!
//! # The match
//!
//! Fuse.js scores a match with the bitap algorithm: `errors / query length` plus
//! `distance from the start / 100`, keeps scores up to the threshold `0.35` and sorts
//! by score, ties in vocabulary order. [`TagIndex::search`] computes the same score
//! with an approximate substring match (Sellers' edit-distance variant of
//! Levenshtein): the fewest insertions, deletions and substitutions that make the query
//! appear somewhere in the tag, at the earliest start among the alignments with that
//! many errors. Text is compared lowercased and character by character. The product
//! expectations of the Solid tests hold (a prefix before a later match, an exact name
//! first among equals by vocabulary order, a two-letter query needs an exact
//! substring); the exact scores of Fuse.js are not reproduced (its bitap counts errors
//! in 32-bit windows and a long query in chunks).
//!
//! # Highlights
//!
//! A suggestion carries the byte ranges of the tag's characters that the best
//! alignment matched to the query, merged where they touch. Fuse.js marks every
//! character of its search window that occurs in the query, so for a fuzzy match its
//! highlight can be wider; for an exact substring both mark the substring.
//!
//! # Cost
//!
//! The index is built once per vocabulary (lowercased characters in one flat array),
//! and a search walks it once in O(tags × tag length × query length) without
//! allocating per tag. A search can be stopped between tags ([`Cancel`]).

use std::{
    collections::{BTreeSet, HashSet},
    ops::Range,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{
    active_token::ActiveToken,
    text::{js_trim, starts_with_ignore_ascii_case},
};

/// Fuse.js `threshold`.
pub const THRESHOLD: f64 = 0.35;
/// Fuse.js `distance`: a start this many characters late costs a whole error ratio.
pub const DISTANCE: f64 = 100.0;
/// Matches considered (`DEFAULT_SEARCH_LIMIT`).
pub const SEARCH_LIMIT: usize = 20;
/// Suggestions shown (`DEFAULT_SUGGESTION_LIMIT`).
pub const SUGGESTION_LIMIT: usize = 8;
/// Tags searched between two looks at the cancel flag.
const CANCEL_STRIDE: usize = 1024;

/// `TagSuggestion`: a tag of the vocabulary and the byte ranges to highlight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSuggestion {
    pub value: String,
    /// Half-open byte ranges of `value`, ascending and disjoint.
    pub ranges: Vec<Range<usize>>,
}

/// A flag that stops a running search; cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

/// Two flags are equal when they are the same flag.
impl PartialEq for Cancel {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Cancel {}

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The search was stopped through its [`Cancel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

/// One match of [`TagIndex::search`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuzzyMatch {
    /// The tag's position in the vocabulary.
    pub index: usize,
    /// `errors / query length + start / 100`; 0 is an exact prefix.
    pub score: f64,
}

/// The vocabulary prepared for searching.
#[derive(Debug)]
pub struct TagIndex {
    tags: Arc<Vec<String>>,
    /// The lowercased characters of every tag, one after the other.
    chars: Vec<char>,
    /// Where each tag's characters start in `chars`; one more entry than tags.
    starts: Vec<u32>,
    /// For each character of `chars`: the byte offset in its tag of the character
    /// it was lowercased from.
    origins: Vec<u32>,
}

impl TagIndex {
    pub fn new(tags: Arc<Vec<String>>) -> Self {
        let size: usize = tags.iter().map(String::len).sum();
        let mut chars = Vec::with_capacity(size);
        let mut origins = Vec::with_capacity(size);
        let mut starts = Vec::with_capacity(tags.len() + 1);
        for tag in tags.iter() {
            starts.push(u32::try_from(chars.len()).unwrap_or(u32::MAX));
            for (at, c) in tag.char_indices() {
                let origin = u32::try_from(at).unwrap_or(u32::MAX);
                for lower in c.to_lowercase() {
                    chars.push(lower);
                    origins.push(origin);
                }
            }
        }
        starts.push(u32::try_from(chars.len()).unwrap_or(u32::MAX));
        Self {
            tags,
            chars,
            starts,
            origins,
        }
    }

    /// The vocabulary this index was built from.
    pub fn tags(&self) -> &Arc<Vec<String>> {
        &self.tags
    }

    pub fn len(&self) -> usize {
        self.tags.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    fn span(&self, index: usize) -> Range<usize> {
        self.starts[index] as usize..self.starts[index + 1] as usize
    }

    /// `fuse.search(query, { limit })`: the best `limit` matches of `query`
    /// (lowercased here), by score and then vocabulary order.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        cancel: &Cancel,
    ) -> Result<Vec<FuzzyMatch>, Cancelled> {
        let pattern: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
        if pattern.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut scratch = Scratch::new(pattern.len());
        let mut found = Vec::new();
        for index in 0..self.tags.len() {
            if index % CANCEL_STRIDE == 0 && cancel.is_cancelled() {
                return Err(Cancelled);
            }
            let text = &self.chars[self.span(index)];
            if let Some(score) = scratch.best_score(&pattern, text) {
                found.push(FuzzyMatch { index, score });
            }
        }
        let order = |a: &FuzzyMatch, b: &FuzzyMatch| {
            a.score.total_cmp(&b.score).then(a.index.cmp(&b.index))
        };
        if found.len() > limit {
            found.select_nth_unstable_by(limit - 1, order);
            found.truncate(limit);
        }
        found.sort_by(order);
        Ok(found)
    }

    /// The byte ranges of the tag at `index` that the best alignment of `query`
    /// matched.
    pub fn highlight(&self, index: usize, query: &str) -> Vec<Range<usize>> {
        let pattern: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
        let span = self.span(index);
        let text = &self.chars[span.clone()];
        let matched = align(&pattern, text);
        let tag = &self.tags[index];
        let byte_end = |position: usize| {
            let origin = self.origins[span.start + position] as usize;
            origin + tag[origin..].chars().next().map_or(0, char::len_utf8)
        };
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for position in matched {
            let start = self.origins[span.start + position] as usize;
            let end = byte_end(position);
            match ranges.last_mut() {
                Some(last) if last.end >= start => last.end = last.end.max(end),
                _ => ranges.push(start..end),
            }
        }
        ranges
    }
}

/// One cell of the edit-distance table: the errors so far and where in the text the
/// alignment starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cell {
    errors: u32,
    start: u32,
}

impl Cell {
    /// The better of two alignments: fewer errors, then the earlier start.
    fn better(self, other: Self) -> Self {
        if (other.errors, other.start) < (self.errors, self.start) {
            other
        } else {
            self
        }
    }
}

fn score(errors: u32, start: u32, pattern_len: usize) -> f64 {
    f64::from(errors) / pattern_len as f64 + f64::from(start) / DISTANCE
}

/// The two columns of the table, reused for every tag.
struct Scratch {
    previous: Vec<Cell>,
    current: Vec<Cell>,
}

impl Scratch {
    fn new(pattern_len: usize) -> Self {
        Self {
            previous: vec![
                Cell {
                    errors: 0,
                    start: 0
                };
                pattern_len + 1
            ],
            current: vec![
                Cell {
                    errors: 0,
                    start: 0
                };
                pattern_len + 1
            ],
        }
    }

    /// The best score of `pattern` anywhere in `text`, if it is within the threshold.
    fn best_score(&mut self, pattern: &[char], text: &[char]) -> Option<f64> {
        let m = pattern.len();
        for (row, cell) in self.previous.iter_mut().enumerate() {
            *cell = Cell {
                errors: row as u32,
                start: 0,
            };
        }
        let mut best: Option<f64> = None;
        for (column, &c) in text.iter().enumerate() {
            let start = column as u32 + 1;
            self.current[0] = Cell { errors: 0, start };
            for row in 1..=m {
                let diagonal = self.previous[row - 1];
                let substitute = Cell {
                    errors: diagonal.errors + u32::from(pattern[row - 1] != c),
                    start: if row == 1 {
                        column as u32
                    } else {
                        diagonal.start
                    },
                };
                let skip_text = Cell {
                    errors: self.previous[row].errors + 1,
                    start: self.previous[row].start,
                };
                let skip_pattern = Cell {
                    errors: self.current[row - 1].errors + 1,
                    start: self.current[row - 1].start,
                };
                self.current[row] = substitute.better(skip_text).better(skip_pattern);
            }
            let end = self.current[m];
            let candidate = score(end.errors, end.start, m);
            if candidate <= THRESHOLD && best.is_none_or(|best| candidate < best) {
                best = Some(candidate);
            }
            std::mem::swap(&mut self.previous, &mut self.current);
        }
        best
    }
}

/// The text positions the best alignment of `pattern` in `text` matches exactly, in
/// ascending order.
fn align(pattern: &[char], text: &[char]) -> Vec<usize> {
    let m = pattern.len();
    let n = text.len();
    if m == 0 || n == 0 {
        return Vec::new();
    }
    // Moves: 0 substitute or match, 1 skip a text character, 2 skip a pattern character.
    let mut table = vec![
        Cell {
            errors: 0,
            start: 0
        };
        (m + 1) * (n + 1)
    ];
    let mut moves = vec![0u8; (m + 1) * (n + 1)];
    let at = |row: usize, column: usize| row * (n + 1) + column;
    for row in 0..=m {
        table[at(row, 0)] = Cell {
            errors: row as u32,
            start: 0,
        };
        moves[at(row, 0)] = 2;
    }
    for column in 1..=n {
        table[at(0, column)] = Cell {
            errors: 0,
            start: column as u32,
        };
        for row in 1..=m {
            let diagonal = table[at(row - 1, column - 1)];
            let options = [
                Cell {
                    errors: diagonal.errors + u32::from(pattern[row - 1] != text[column - 1]),
                    start: if row == 1 {
                        column as u32 - 1
                    } else {
                        diagonal.start
                    },
                },
                Cell {
                    errors: table[at(row, column - 1)].errors + 1,
                    start: table[at(row, column - 1)].start,
                },
                Cell {
                    errors: table[at(row - 1, column)].errors + 1,
                    start: table[at(row - 1, column)].start,
                },
            ];
            let mut chosen = 0;
            for (option, cell) in options.iter().enumerate().skip(1) {
                if options[chosen].better(*cell) != options[chosen] {
                    chosen = option;
                }
            }
            table[at(row, column)] = options[chosen];
            moves[at(row, column)] = chosen as u8;
        }
    }
    // The best end; among ends of equal score the last, whose alignment matches the
    // most characters (`cart` for `cat` highlights c, a and t).
    let Some(end) = (1..=n)
        .map(|column| (column, table[at(m, column)]))
        .fold(
            None,
            |best: Option<(usize, Cell)>, (column, cell)| match best {
                Some((_, kept))
                    if score(cell.errors, cell.start, m) > score(kept.errors, kept.start, m) =>
                {
                    best
                }
                _ => Some((column, cell)),
            },
        )
        .map(|(column, _)| column)
    else {
        return Vec::new();
    };
    let (mut row, mut column) = (m, end);
    let mut matched = Vec::new();
    while row > 0 && column > 0 {
        match moves[at(row, column)] {
            0 => {
                if pattern[row - 1] == text[column - 1] {
                    matched.push(column - 1);
                }
                row -= 1;
                column -= 1;
            }
            1 => column -= 1,
            _ => row -= 1,
        }
    }
    matched.reverse();
    matched
}

/// `/^gn(?::.*)?$/i` or `/^tags(?::.*)?$/i`: a query that reads as an operator.
pub fn is_meta_filter_query(query: &str) -> bool {
    let operator = |word: &str| {
        starts_with_ignore_ascii_case(query, word)
            && (query.len() == word.len() || query[word.len()..].starts_with(':'))
    };
    operator("gn") || operator("tags")
}

/// `buildTagSuggestions` with the default limits ([`SEARCH_LIMIT`],
/// [`SUGGESTION_LIMIT`]).
pub fn build_tag_suggestions(
    token: Option<&ActiveToken>,
    used: &BTreeSet<String>,
    excluded: &HashSet<String>,
    index: &TagIndex,
    cancel: &Cancel,
) -> Result<Vec<TagSuggestion>, Cancelled> {
    build_tag_suggestions_with(
        token,
        used,
        excluded,
        index,
        SEARCH_LIMIT,
        SUGGESTION_LIMIT,
        cancel,
    )
}

/// `buildTagSuggestions`: the matches of the token's query, without hard exclusions
/// (removed before the search limit counts), without tags already used in the draft
/// unless they equal the query, without duplicates, at most `suggestion_limit`. An
/// unquoted positive operator (`tags`, `tags:3`, `gn`, `gn:x`) gets none.
pub fn build_tag_suggestions_with(
    token: Option<&ActiveToken>,
    used: &BTreeSet<String>,
    excluded: &HashSet<String>,
    index: &TagIndex,
    search_limit: usize,
    suggestion_limit: usize,
    cancel: &Cancel,
) -> Result<Vec<TagSuggestion>, Cancelled> {
    let Some(token) = token else {
        return Ok(Vec::new());
    };
    let query = js_trim(&token.query).to_lowercase();
    if !token.literal && !token.negative && is_meta_filter_query(&query) {
        return Ok(Vec::new());
    }
    let matches = index.search(&query, search_limit + excluded.len(), cancel)?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut suggestions = Vec::new();
    let mut considered = 0;
    for found in matches {
        let value = &index.tags()[found.index];
        let normalized = js_trim(value).to_lowercase();
        if excluded.contains(&normalized) {
            continue;
        }
        if considered >= search_limit {
            break;
        }
        considered += 1;
        if used.contains(&normalized) && normalized != query {
            continue;
        }
        if !seen.insert(normalized) {
            continue;
        }
        suggestions.push(TagSuggestion {
            value: value.clone(),
            ranges: index.highlight(found.index, &query),
        });
        if suggestions.len() >= suggestion_limit {
            break;
        }
    }
    Ok(suggestions)
}

#[cfg(test)]
mod tests;
