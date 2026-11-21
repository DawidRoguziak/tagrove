//! Whether an accepted metadata write has to restart the query
//! (`src/library/metadata/queryImpact.ts`).
//!
//! The backend reports what a tag write touched as a `TagQueryImpact`; the applied
//! filters decide whether the current result can have changed.

use media_tagger_core::models::TagQueryImpact;

use super::filters::LibraryFilters;
use crate::backend::MetaFilter;

/// `tagMutationRequiresRefresh`: `None` keeps the query, `All` restarts it, and `Tags`
/// restarts it when a changed tag is one of the applied include or exclude tags, or
/// when a tag count changed under an exact-count filter (`tags`, `tags:N`).
pub fn tag_mutation_requires_refresh(impact: &TagQueryImpact, filters: &LibraryFilters) -> bool {
    match impact {
        TagQueryImpact::None => false,
        TagQueryImpact::All => true,
        TagQueryImpact::Tags {
            changed_tags,
            tag_count_changed,
        } => {
            if *tag_count_changed
                && matches!(filters.meta_filter, Some(MetaFilter::HasNoTags { .. }))
            {
                return true;
            }
            let applied: Vec<String> = filters
                .include
                .iter()
                .chain(&filters.exclude)
                .map(|tag| tag.trim().to_lowercase())
                .collect();
            changed_tags.iter().any(|tag| applied.contains(tag))
        }
    }
}

/// `favoriteMutationRequiresRefresh`: only removing a favorite from a favorites-only
/// result changes it.
pub fn favorite_mutation_requires_refresh(
    applied_favorites_only: bool,
    next_favorite: bool,
) -> bool {
    applied_favorites_only && !next_favorite
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::TagQueryImpact;

    use super::{favorite_mutation_requires_refresh, tag_mutation_requires_refresh};
    use crate::{backend::MetaFilter, library::LibraryFilters};

    fn tags(changed: &[&str], tag_count_changed: bool) -> TagQueryImpact {
        TagQueryImpact::Tags {
            changed_tags: changed.iter().map(|tag| (*tag).to_owned()).collect(),
            tag_count_changed,
        }
    }

    #[test]
    fn none_keeps_and_all_restarts_whatever_the_filters() {
        let filters = LibraryFilters {
            include: vec!["cat".into()],
            ..LibraryFilters::default()
        };

        assert!(!tag_mutation_requires_refresh(
            &TagQueryImpact::None,
            &filters
        ));
        assert!(tag_mutation_requires_refresh(
            &TagQueryImpact::All,
            &filters
        ));
        assert!(tag_mutation_requires_refresh(
            &TagQueryImpact::All,
            &LibraryFilters::default()
        ));
    }

    #[test]
    fn changed_tags_restart_only_when_the_filters_name_one() {
        let filters = LibraryFilters {
            include: vec!["  Cat ".into()],
            exclude: vec!["DOG".into()],
            ..LibraryFilters::default()
        };

        assert!(tag_mutation_requires_refresh(
            &tags(&["cat"], false),
            &filters
        ));
        assert!(tag_mutation_requires_refresh(
            &tags(&["bird", "dog"], true),
            &filters
        ));
        assert!(!tag_mutation_requires_refresh(
            &tags(&["bird"], true),
            &filters
        ));
        assert!(!tag_mutation_requires_refresh(
            &tags(&["cat"], true),
            &LibraryFilters::default()
        ));
    }

    #[test]
    fn a_count_change_restarts_an_exact_count_filter_only() {
        let untagged = LibraryFilters {
            meta_filter: Some(MetaFilter::HasNoTags { tag_count: 0 }),
            ..LibraryFilters::default()
        };
        let grouped = LibraryFilters {
            meta_filter: Some(MetaFilter::GroupName {
                group_name: "trip".into(),
            }),
            ..LibraryFilters::default()
        };

        assert!(tag_mutation_requires_refresh(
            &tags(&["bird"], true),
            &untagged
        ));
        assert!(!tag_mutation_requires_refresh(
            &tags(&["bird"], false),
            &untagged
        ));
        assert!(!tag_mutation_requires_refresh(
            &tags(&["bird"], true),
            &grouped
        ));
    }

    #[test]
    fn only_removing_a_favorite_from_a_favorites_only_result_restarts() {
        assert!(favorite_mutation_requires_refresh(true, false));
        assert!(!favorite_mutation_requires_refresh(true, true));
        assert!(!favorite_mutation_requires_refresh(false, false));
        assert!(!favorite_mutation_requires_refresh(false, true));
    }

    /// queryImpact.test.ts "checks actual membership/count changes", row by row. The
    /// applied filters are what `buildFilterDescriptor` makes of each input (the
    /// search grammar is ported by a later epic): `tags:2` is `HasNoTags { 2 }`,
    /// `gN:album` is `GroupName`.
    #[test]
    fn spec_tag_impact_table_of_the_solid_tests() {
        let include = |tags: &[&str]| LibraryFilters {
            include: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            ..LibraryFilters::default()
        };
        let cat = include(&["cat"]);
        let not_dog = LibraryFilters {
            exclude: vec!["dog".into()],
            ..LibraryFilters::default()
        };
        let cat_not_dog = LibraryFilters {
            exclude: vec!["dog".into()],
            ..include(&["cat"])
        };
        let two_tags = LibraryFilters {
            meta_filter: Some(MetaFilter::HasNoTags { tag_count: 2 }),
            ..LibraryFilters::default()
        };
        let group = LibraryFilters {
            meta_filter: Some(MetaFilter::GroupName {
                group_name: "album".into(),
            }),
            ..LibraryFilters::default()
        };
        let unicode = include(&["żółć|%_:'新"]);
        let cases: [(&str, &LibraryFilters, &[&str], bool, bool); 8] = [
            ("cat", &cat, &["cat"], false, true),
            ("-dog", &not_dog, &["dog"], false, true),
            ("cat -dog", &cat_not_dog, &["unrelated"], true, false),
            ("tags:2", &two_tags, &["cat", "dog"], false, false),
            ("tags:2", &two_tags, &["cat"], true, true),
            ("", &LibraryFilters::default(), &["cat"], true, false),
            ("gN:album", &group, &["cat"], true, false),
            ("żółć|%_:'新", &unicode, &["żółć|%_:'新"], false, true),
        ];
        for (input, filters, changed, count, expected) in cases {
            // The Solid test applies these with media kind "image" and favorites only.
            let filters = LibraryFilters {
                media_kind: Some(crate::backend::MediaKind::Image),
                favorites_only: true,
                ..filters.clone()
            };
            assert_eq!(
                tag_mutation_requires_refresh(&tags(changed, count), &filters),
                expected,
                "{input}"
            );
        }
        // "handles none/all" under each of these inputs.
        for filters in [&cat, &not_dog, &cat_not_dog, &two_tags, &group] {
            assert!(!tag_mutation_requires_refresh(
                &TagQueryImpact::None,
                filters
            ));
            assert!(tag_mutation_requires_refresh(&TagQueryImpact::All, filters));
        }
    }
}
