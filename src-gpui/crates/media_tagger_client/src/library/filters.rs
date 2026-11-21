//! The applied filters of the library: what a query session is started with.
//!
//! The search model (a later epic) owns the draft the user edits; only a submit turns
//! it into these applied filters. The backend normalizes every field again.

use crate::backend::{AssetQueryRequest, MediaKind, MetaFilter};

/// `LibraryFilters` of `createAssetQuery.ts`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryFilters {
    /// Tags every result has (`filterInclude`).
    pub include: Vec<String>,
    /// Tags no result has (`filterExclude`).
    pub exclude: Vec<String>,
    pub meta_filter: Option<MetaFilter>,
    /// `None`: every kind (`appliedMediaKind: "all"`).
    pub media_kind: Option<MediaKind>,
    pub favorites_only: bool,
}

impl LibraryFilters {
    /// The `start_asset_query` request of these filters.
    pub fn request(
        &self,
        owner_id: &str,
        generation: u64,
        page_size: usize,
        anchor_offset: Option<usize>,
    ) -> AssetQueryRequest {
        AssetQueryRequest {
            tags_and: self.include.clone(),
            tags_not: self.exclude.clone(),
            kind: self.media_kind,
            favorites_only: self.favorites_only,
            meta_filter: self.meta_filter.clone(),
            owner_id: owner_id.to_owned(),
            generation,
            page_size,
            anchor_offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LibraryFilters;
    use crate::backend::{AssetQueryRequest, MediaKind, MetaFilter};

    #[test]
    fn default_filters_ask_for_the_whole_library() {
        assert_eq!(
            LibraryFilters::default().request("owner", 4, 128, None),
            AssetQueryRequest::new("owner", 4)
        );
    }

    #[test]
    fn every_filter_field_reaches_the_request() {
        let filters = LibraryFilters {
            include: vec!["cat".into()],
            exclude: vec!["dog".into()],
            meta_filter: Some(MetaFilter::GroupName {
                group_name: "Trip-2026".into(),
            }),
            media_kind: Some(MediaKind::Image),
            favorites_only: true,
        };

        let request = filters.request("owner", 1, 2, Some(31));

        assert_eq!(request.tags_and, ["cat"]);
        assert_eq!(request.tags_not, ["dog"]);
        assert_eq!(request.kind, Some(MediaKind::Image));
        assert!(request.favorites_only);
        assert_eq!(
            request.meta_filter,
            Some(MetaFilter::GroupName {
                group_name: "Trip-2026".into()
            })
        );
        assert_eq!((request.page_size, request.anchor_offset), (2, Some(31)));
    }
}
