//! Search filters as the user edits them and as they are applied
//! (`src/features/search/filterService.ts`).

use std::cmp::Ordering;

use super::{grammar::parse_search_filter, text::js_trim};
use crate::{
    backend::{MediaKind, MetaFilter},
    library::LibraryFilters,
};

/// The media kinds of the segmented control, in its order: `None` is "all".
pub const MEDIA_KIND_OPTIONS: [Option<MediaKind>; 4] = [
    None,
    Some(MediaKind::Image),
    Some(MediaKind::Gif),
    Some(MediaKind::Video),
];

/// The value of a media kind option: `all`, `image`, `gif` or `video`.
pub fn media_kind_value(kind: Option<MediaKind>) -> &'static str {
    kind.map_or("all", MediaKind::as_str)
}

/// The option of a value of [`media_kind_value`]; `None` for an unknown value.
pub fn parse_media_kind_value(value: &str) -> Option<Option<MediaKind>> {
    match value {
        "all" => Some(None),
        other => MediaKind::parse(other).map(Some),
    }
}

/// The label key of a media kind option (`search.mediaKinds.<value>`).
pub fn media_kind_label_key(kind: Option<MediaKind>) -> &'static str {
    match kind {
        None => "search.mediaKinds.all",
        Some(MediaKind::Image) => "search.mediaKinds.image",
        Some(MediaKind::Gif) => "search.mediaKinds.gif",
        Some(MediaKind::Video) => "search.mediaKinds.video",
    }
}

/// `SearchFilters`: what the header shows and the user edits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilters {
    /// The text of the search field.
    pub filter_input: String,
    /// `None`: every kind.
    pub media_kind: Option<MediaKind>,
    pub favorites_only: bool,
}

/// `FilterDescriptor`: the normalized form of filters, compared to tell whether the
/// applied filters changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterDescriptor {
    /// Trimmed, lowercased, deduplicated and sorted as JavaScript sorts strings (by
    /// UTF-16 code units).
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// A group name is trimmed and lowercased.
    pub meta_filter: Option<MetaFilter>,
    pub media_kind: Option<MediaKind>,
    pub favorites_only: bool,
}

impl FilterDescriptor {
    /// The filters a query is started with.
    pub fn library_filters(&self) -> LibraryFilters {
        LibraryFilters {
            include: self.include.clone(),
            exclude: self.exclude.clone(),
            meta_filter: self.meta_filter.clone(),
            media_kind: self.media_kind,
            favorites_only: self.favorites_only,
        }
    }
}

/// `Array.prototype.sort` without a comparator: by UTF-16 code units.
fn js_string_order(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn normalize_side(tags: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = tags.iter().map(|tag| js_trim(tag).to_lowercase()).collect();
    tags.sort_by(|left, right| js_string_order(left, right));
    tags.dedup();
    tags
}

/// `buildFilterDescriptor`. Invalid input yields the descriptor of no tag filter, as in
/// Solid; a submit validates before it builds one.
pub fn build_filter_descriptor(filters: &SearchFilters) -> FilterDescriptor {
    let parsed = parse_search_filter(&filters.filter_input);
    FilterDescriptor {
        include: normalize_side(&parsed.include),
        exclude: normalize_side(&parsed.exclude),
        meta_filter: parsed.meta_filter.map(|meta| match meta {
            MetaFilter::GroupName { group_name } => MetaFilter::GroupName {
                group_name: js_trim(&group_name).to_lowercase(),
            },
            other => other,
        }),
        media_kind: filters.media_kind,
        favorites_only: filters.favorites_only,
    }
}

/// `buildMetaFilterKey`.
pub fn meta_filter_key(meta_filter: Option<&MetaFilter>) -> String {
    match meta_filter {
        None => String::new(),
        Some(MetaFilter::HasNoTags { tag_count }) => format!("hasNoTags:{tag_count}"),
        Some(MetaFilter::GroupName { group_name }) => {
            format!("groupName:{}", js_trim(group_name).to_lowercase())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_filter_descriptor, js_string_order, media_kind_value, parse_media_kind_value,
        SearchFilters, MEDIA_KIND_OPTIONS,
    };
    use std::cmp::Ordering;

    #[test]
    fn media_kind_values_round_trip() {
        for kind in MEDIA_KIND_OPTIONS {
            assert_eq!(parse_media_kind_value(media_kind_value(kind)), Some(kind));
        }
        assert_eq!(parse_media_kind_value("audio"), None);
    }

    #[test]
    fn tags_sort_by_utf16_code_units() {
        // U+FF5E sorts after U+1F600 by code point, before it by UTF-16 units.
        assert_eq!(js_string_order("\u{FF5E}", "\u{1F600}"), Ordering::Greater);
        let descriptor = build_filter_descriptor(&SearchFilters {
            filter_input: "Dog cat dog -B -a".into(),
            ..SearchFilters::default()
        });
        assert_eq!(descriptor.include, ["cat", "dog"]);
        assert_eq!(descriptor.exclude, ["a", "b"]);
    }
}
