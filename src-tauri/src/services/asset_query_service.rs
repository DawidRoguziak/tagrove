use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use crate::{
    db::{self, AssetMetaFilter},
    error::AppResult,
    models::{
        AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySection,
        AssetQuerySectionsResult, StartAssetQueryResult,
    },
    services::db_pool::OperationPermit,
};

const MAX_SESSIONS: usize = 4;
const MAX_RETAINED_ID_BYTES: usize = 64 * 1024 * 1024;
/// Idle expiry of a session. Every lookup by key or ID (start, page, position,
/// ID range, sections, selection resolve) restarts it. While a range selection
/// holds a session, the frontend reads one ID of it at once and then every
/// `SESSION_KEEPALIVE_MS` (`src/features/gallery/createSelection.ts`, one
/// minute), which must stay well below this (ADR 0029).
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);
/// Largest asset ID that a binary ID range can carry exactly as an f64.
const MAX_EXACT_F64_ID: i64 = 1 << 53;

/// A bulk selection expressed against one query session (stage 09). `ranges`
/// are inclusive `[start, end]` positions in the session vector, ascending and
/// non-overlapping; `excluded` are IDs inside those ranges that are not
/// selected, and `included` are selected IDs outside them (possibly from an
/// earlier query).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSpec {
    pub session_id: u64,
    pub ranges: Vec<[usize; 2]>,
    #[serde(default)]
    pub excluded: Vec<i64>,
    #[serde(default)]
    pub included: Vec<i64>,
}

/// The asset IDs a `SelectionSpec` denotes, in session order followed by the
/// remaining `included` IDs, plus what is needed to prove inside a write
/// transaction that the session snapshot is still current.
#[derive(Debug)]
pub struct ResolvedSelection {
    pub asset_ids: Vec<i64>,
    dependencies: Vec<String>,
    dependency_revision: i64,
}

impl ResolvedSelection {
    /// Whether no write has changed the session's query result since it was
    /// built. Call it inside the write transaction that applies the selection.
    pub fn is_current(&self, conn: &rusqlite::Connection) -> anyhow::Result<bool> {
        Ok(db::query_dependency_revision(conn, &self.dependencies)? == self.dependency_revision)
    }
}

#[derive(Debug, Clone)]
pub struct AssetQueryFilters {
    pub tags_and: Vec<String>,
    pub tags_not: Vec<String>,
    pub kind: Option<String>,
    pub favorites_only: bool,
    pub meta_filter: Option<AssetMetaFilter>,
}

impl AssetQueryFilters {
    fn dependencies(&self) -> Vec<String> {
        let mut names = vec!["general".into()];
        if self.favorites_only {
            names.push("favorites".into());
        }
        if matches!(self.meta_filter, Some(AssetMetaFilter::HasNoTags { .. })) {
            names.push("tag_count".into());
        }
        names.extend(
            self.tags_and
                .iter()
                .chain(&self.tags_not)
                .map(|tag| format!("tag:{tag}")),
        );
        names.sort();
        names.dedup();
        names
    }
}

#[derive(Clone)]
struct QuerySession {
    id: u64,
    dependency_revision: i64,
    dependencies: Vec<String>,
    key: String,
    asset_ids: Arc<Vec<i64>>,
    /// Filled by a filtered build, or on the first sections read (shared by
    /// clones of the session).
    sections: Arc<OnceLock<Vec<AssetQuerySection>>>,
    last_accessed: Instant,
}

#[derive(Default)]
struct QueryCache {
    sessions: VecDeque<QuerySession>,
    epoch: u64,
}

pub struct AssetQueryManager {
    next_session_id: AtomicU64,
    latest_request: Arc<AtomicU64>,
    cache: Mutex<QueryCache>,
    owner_generations: Mutex<HashMap<String, u64>>,
    max_id_bytes: usize,
    #[cfg(test)]
    after_revision_read: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl AssetQueryManager {
    pub fn new() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            latest_request: Arc::new(AtomicU64::new(0)),
            cache: Mutex::new(QueryCache::default()),
            owner_generations: Mutex::new(HashMap::new()),
            max_id_bytes: MAX_RETAINED_ID_BYTES,
            #[cfg(test)]
            after_revision_read: Mutex::new(None),
        }
    }

    /// Admission can run out of IPC order. Only a newer generation from the
    /// same frontend owner may replace its previous request. Different owners
    /// have independent counters, including after a frontend remount.
    pub fn begin_request(&self, owner_id: &str, generation: u64) -> u64 {
        let mut owners = self
            .owner_generations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if owners
            .get(owner_id)
            .is_some_and(|previous| generation <= *previous)
        {
            return 0; // Reserved obsolete token; admitted tokens start at one.
        }
        owners.insert(owner_id.to_owned(), generation);
        self.latest_request.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn is_superseded(&self, request_id: u64, _generation: u64) -> bool {
        self.latest_request.load(Ordering::SeqCst) != request_id
    }

    pub fn start(
        &self,
        permit: &OperationPermit,
        filters: AssetQueryFilters,
        page_size: usize,
        request_id: u64,
        generation: u64,
    ) -> AppResult<StartAssetQueryResult> {
        self.start_at(permit, filters, page_size, request_id, generation, None)
    }

    /// Starts a query whose `ready` result carries the page that contains
    /// `anchor_offset` (clamped to the last row) instead of the first page.
    pub fn start_at(
        &self,
        permit: &OperationPermit,
        filters: AssetQueryFilters,
        page_size: usize,
        request_id: u64,
        generation: u64,
        anchor_offset: Option<usize>,
    ) -> AppResult<StartAssetQueryResult> {
        let result = self.start_inner(
            permit,
            filters,
            page_size,
            request_id,
            generation,
            anchor_offset,
        );
        if self.is_superseded(request_id, generation) {
            Ok(StartAssetQueryResult::Superseded)
        } else {
            result
        }
    }

    fn start_inner(
        &self,
        permit: &OperationPermit,
        filters: AssetQueryFilters,
        page_size: usize,
        request_id: u64,
        generation: u64,
        anchor_offset: Option<usize>,
    ) -> AppResult<StartAssetQueryResult> {
        let epoch = self.cache.lock().unwrap_or_else(|e| e.into_inner()).epoch;
        let conn = permit.connection()?;
        let latest = self.latest_request.clone();
        conn.progress_handler(
            1000,
            Some(move || latest.load(Ordering::SeqCst) != request_id),
        );
        // One deferred read transaction gives revision, ordered IDs, and the
        // first page a single consistent SQLite snapshot.
        let tx = conn.unchecked_transaction()?;
        let revision = db::current_library_revision(&tx)?;
        #[cfg(test)]
        self.after_revision_read();
        let dependencies = filters.dependencies();
        let dependency_revision = db::query_dependency_revision(&tx, &dependencies)?;
        let key = filter_key(&filters, dependency_revision);
        let page_size = page_size.clamp(1, 256);
        // An obsolete request must never observe a ready result, including on
        // the cheap cache-hit path.
        if self.is_superseded(request_id, generation) {
            return Ok(StartAssetQueryResult::Superseded);
        }

        if let Some(session) = self.cached_session(&key) {
            let (offset, ids) = anchor_page(&session.asset_ids, page_size, anchor_offset);
            let items = db::list_asset_summaries_by_ids(&tx, ids)?;
            return Ok(StartAssetQueryResult::Ready {
                session_id: session.id,
                revision,
                total: session.asset_ids.len(),
                offset,
                items,
            });
        }

        let cancel_check = || self.is_superseded(request_id, generation);
        let started = Instant::now();
        let ids_result = db::try_list_ordered_asset_ids_with_meta(
            &tx,
            &filters.tags_and,
            &filters.tags_not,
            filters.kind.as_deref(),
            filters.favorites_only,
            filters.meta_filter.as_ref(),
            cancel_check,
        );
        if self.is_superseded(request_id, generation) {
            return Ok(StartAssetQueryResult::Superseded);
        }
        let Some(ordered) = ids_result? else {
            perf_log("asset-query-cancelled", started, 0);
            return Ok(StartAssetQueryResult::Superseded);
        };
        perf_log("asset-query-build", started, ordered.ids.len());

        if self.is_superseded(request_id, generation) {
            return Ok(StartAssetQueryResult::Superseded);
        }

        let session = QuerySession {
            id: self.next_session_id.fetch_add(1, Ordering::Relaxed),
            dependency_revision,
            dependencies,
            key,
            asset_ids: Arc::new(ordered.ids),
            sections: Arc::new(ordered.sections.map(OnceLock::from).unwrap_or_default()),
            last_accessed: Instant::now(),
        };
        let (offset, ids) = anchor_page(&session.asset_ids, page_size, anchor_offset);
        let items = db::list_asset_summaries_by_ids(&tx, ids)?;
        let total = session.asset_ids.len();
        let session_id = session.id;
        drop(tx);
        if !self.insert_session(session, request_id, epoch)? {
            return Ok(StartAssetQueryResult::Superseded);
        }

        Ok(StartAssetQueryResult::Ready {
            session_id,
            revision,
            total,
            offset,
            items,
        })
    }

    /// Ordered session IDs for the inclusive range `start..=end`, clamped to
    /// the snapshot, encoded for a binary IPC response (see
    /// `encode_id_range`). `None` means the session is stale. No SQL reads
    /// the IDs; only the dependency revision is checked.
    pub fn id_range(
        &self,
        permit: &OperationPermit,
        session_id: u64,
        start: usize,
        end: usize,
    ) -> AppResult<Option<Vec<u8>>> {
        let conn = permit.connection()?;
        let tx = conn.unchecked_transaction()?;
        #[cfg(test)]
        self.after_revision_read();
        let Some(session) = self.session_by_id(session_id) else {
            return Ok(None);
        };
        if session.dependency_revision != db::query_dependency_revision(&tx, &session.dependencies)?
        {
            self.remove_session(session_id);
            return Ok(None);
        }
        drop(tx);
        let total = session.asset_ids.len();
        let ids = if start > end || start >= total {
            &session.asset_ids[..0]
        } else {
            &session.asset_ids[start..=end.min(total - 1)]
        };
        encode_id_range(ids).map(Some)
    }

    /// Timeline sections of a session (stage 08), checked against the
    /// dependency revision like pages. A filtered session built them with its
    /// IDs; an unfiltered one reads them once, here, from the sort-key index in
    /// this read transaction, keeping that scan off the rebuild path. When the
    /// live index no longer matches the session's row count, the result is
    /// `Ready` with no sections and the session stays usable (ADR 0027).
    pub fn sections(
        &self,
        permit: &OperationPermit,
        session_id: u64,
    ) -> AppResult<AssetQuerySectionsResult> {
        let conn = permit.connection()?;
        let tx = conn.unchecked_transaction()?;
        #[cfg(test)]
        self.after_revision_read();
        let Some(session) = self.session_by_id(session_id) else {
            return Ok(AssetQuerySectionsResult::Stale);
        };
        if session.dependency_revision != db::query_dependency_revision(&tx, &session.dependencies)?
        {
            self.remove_session(session_id);
            return Ok(AssetQuerySectionsResult::Stale);
        }
        let total = session.asset_ids.len();
        if session.sections.get().is_none() {
            let started = Instant::now();
            let sections = db::list_unfiltered_query_sections(&tx)?;
            perf_log("asset-query-sections", started, sections.len());
            // A scan commits batches between its throttled revision bumps
            // (ADR 0002), so the table can hold rows this session's snapshot
            // does not while the dependency revision still matches. Those
            // sections do not describe the session, but its IDs and pages are
            // still the snapshot the revision promises: keep the session,
            // return no sections, and store nothing. The scan's next bump
            // (throttled, final, or error-path) makes the session stale, and
            // the session built after it reads its sections again (ADR 0027).
            if sections.iter().map(|section| section.count).sum::<usize>() != total {
                perf_log("asset-query-sections-mismatch", started, total);
                return Ok(AssetQuerySectionsResult::Ready {
                    session_id,
                    total,
                    sections: Vec::new(),
                });
            }
            let _ = session.sections.set(sections);
        }
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total,
            sections: session.sections.get().cloned().unwrap_or_default(),
        })
    }

    /// Resolves a selection spec on its session vector without SQL. `None`
    /// means the session is gone (evicted, expired, or removed as stale); a
    /// malformed spec is an error. The caller must still check
    /// `ResolvedSelection::is_current` in its write transaction.
    pub fn resolve_selection(&self, spec: &SelectionSpec) -> AppResult<Option<ResolvedSelection>> {
        let Some(session) = self.session_by_id(spec.session_id) else {
            return Ok(None);
        };
        let ids = &session.asset_ids;
        let mut previous_end = None;
        let mut covered = 0usize;
        for &[start, end] in &spec.ranges {
            if start > end || end >= ids.len() || previous_end.is_some_and(|last| start <= last) {
                return Err("Selection ranges are invalid. Select the items again.".into());
            }
            previous_end = Some(end);
            covered += end - start + 1;
        }
        let excluded: HashSet<i64> = spec.excluded.iter().copied().collect();
        let mut included: HashSet<i64> = spec.included.iter().copied().collect();
        let mut asset_ids =
            Vec::with_capacity(covered.saturating_sub(excluded.len()) + included.len());
        for &[start, end] in &spec.ranges {
            let slice = &ids[start..=end];
            if excluded.is_empty() && included.is_empty() {
                asset_ids.extend_from_slice(slice);
                continue;
            }
            for &id in slice {
                // An ID listed as included that is also covered is selected once.
                included.remove(&id);
                if !excluded.contains(&id) {
                    asset_ids.push(id);
                }
            }
        }
        // Spec order for the IDs outside the ranges, without duplicates.
        let mut seen = HashSet::new();
        asset_ids.extend(
            spec.included
                .iter()
                .copied()
                .filter(|id| included.contains(id) && seen.insert(*id)),
        );
        Ok(Some(ResolvedSelection {
            asset_ids,
            dependencies: session.dependencies.clone(),
            dependency_revision: session.dependency_revision,
        }))
    }

    pub fn page(
        &self,
        permit: &OperationPermit,
        session_id: u64,
        offset: usize,
        limit: usize,
    ) -> AppResult<AssetQueryPageResult> {
        let conn = permit.connection()?;
        let tx = conn.unchecked_transaction()?;
        let revision = db::current_library_revision(&tx)?;
        #[cfg(test)]
        self.after_revision_read();
        let Some(session) = self.session_by_id(session_id) else {
            return Ok(AssetQueryPageResult::Stale);
        };
        if session.dependency_revision != db::query_dependency_revision(&tx, &session.dependencies)?
        {
            self.remove_session(session_id);
            return Ok(AssetQueryPageResult::Stale);
        }

        let start = offset.min(session.asset_ids.len());
        let end = start
            .saturating_add(limit.clamp(1, 256))
            .min(session.asset_ids.len());
        let items = db::list_asset_summaries_by_ids(&tx, &session.asset_ids[start..end])?;
        Ok(AssetQueryPageResult::Ready {
            session_id,
            revision,
            total: session.asset_ids.len(),
            offset: start,
            items,
        })
    }

    pub fn position(
        &self,
        permit: &OperationPermit,
        session_id: u64,
        asset_id: i64,
    ) -> AppResult<AssetQueryPositionResult> {
        let conn = permit.connection()?;
        let tx = conn.unchecked_transaction()?;
        let _revision = db::current_library_revision(&tx)?;
        #[cfg(test)]
        self.after_revision_read();
        let Some(session) = self.session_by_id(session_id) else {
            return Ok(AssetQueryPositionResult::Stale);
        };
        if session.dependency_revision != db::query_dependency_revision(&tx, &session.dependencies)?
        {
            self.remove_session(session_id);
            return Ok(AssetQueryPositionResult::Stale);
        }
        Ok(
            match session.asset_ids.iter().position(|id| *id == asset_id) {
                Some(index) => AssetQueryPositionResult::Resolved { index },
                None => AssetQueryPositionResult::Missing,
            },
        )
    }

    fn cached_session(&self, key: &str) -> Option<QuerySession> {
        let mut cache = self.cache.lock().ok()?;
        prune(&mut cache);
        let index = cache
            .sessions
            .iter()
            .position(|session| session.key == key)?;
        let mut session = cache.sessions.remove(index)?;
        session.last_accessed = Instant::now();
        cache.sessions.push_front(session.clone());
        Some(session)
    }

    fn session_by_id(&self, id: u64) -> Option<QuerySession> {
        let mut cache = self.cache.lock().ok()?;
        prune(&mut cache);
        let index = cache.sessions.iter().position(|session| session.id == id)?;
        let mut session = cache.sessions.remove(index)?;
        session.last_accessed = Instant::now();
        cache.sessions.push_front(session.clone());
        Some(session)
    }

    fn insert_session(
        &self,
        session: QuerySession,
        request_id: u64,
        epoch: u64,
    ) -> AppResult<bool> {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.epoch != epoch || self.latest_request.load(Ordering::SeqCst) != request_id {
                return Ok(false);
            }
            if session.asset_ids.capacity() * std::mem::size_of::<i64>() > self.max_id_bytes {
                return Err("Query exceeds the retained session memory limit. Narrow your search and try again.".into());
            }
            prune(&mut cache);
            cache
                .sessions
                .retain(|existing| existing.key != session.key);
            cache.sessions.push_front(session);
            cache.sessions.truncate(MAX_SESSIONS);
            while cache
                .sessions
                .iter()
                .map(|s| s.asset_ids.capacity() * std::mem::size_of::<i64>())
                .sum::<usize>()
                > self.max_id_bytes
            {
                cache.sessions.pop_back();
            }
            return Ok(true);
        }
        Err("Query cache is unavailable. Retry your search.".into())
    }

    fn remove_session(&self, id: u64) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.sessions.retain(|session| session.id != id);
        }
    }

    #[cfg(test)]
    fn after_revision_read(&self) {
        let hook = self.after_revision_read.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
    }

    pub fn clear(&self) {
        self.latest_request.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut cache) = self.cache.lock() {
            cache.epoch += 1;
            cache.sessions.clear();
        }
    }
}

fn prune(cache: &mut QueryCache) {
    let now = Instant::now();
    cache
        .sessions
        .retain(|session| now.duration_since(session.last_accessed) <= SESSION_TTL);
}

/// The aligned page containing `anchor` (clamped to the last row), as its
/// offset and ID slice. Without an anchor this is the first page.
fn anchor_page(ids: &[i64], page_size: usize, anchor: Option<usize>) -> (usize, &[i64]) {
    let anchor = anchor.unwrap_or(0).min(ids.len().saturating_sub(1));
    let offset = anchor / page_size * page_size;
    let end = offset.saturating_add(page_size).min(ids.len());
    (offset, &ids[offset.min(end)..end])
}

/// Binary ID range: little-endian f64 values `[count, id_0, .., id_{count-1}]`.
/// A zero-length buffer is reserved for `stale`, so a ready empty range is
/// eight bytes. IDs above 2^53 cannot round-trip through f64 and are rejected.
fn encode_id_range(ids: &[i64]) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::with_capacity((ids.len() + 1) * 8);
    bytes.extend_from_slice(&(ids.len() as f64).to_le_bytes());
    for &id in ids {
        if !(0..=MAX_EXACT_F64_ID).contains(&id) {
            return Err(format!("Asset ID {id} cannot be sent as an exact range value").into());
        }
        bytes.extend_from_slice(&(id as f64).to_le_bytes());
    }
    Ok(bytes)
}

fn filter_key(filters: &AssetQueryFilters, revision: i64) -> String {
    format!(
        "{revision}|{:?}|{:?}|{:?}|{}|{:?}",
        filters.tags_and,
        filters.tags_not,
        filters.kind,
        filters.favorites_only,
        filters.meta_filter
    )
}

fn perf_log(name: &str, started: Instant, items: usize) {
    if std::env::var_os("MEDIATAGGER_PERF").is_some() {
        eprintln!(
            "[perf] name={name} elapsed_ms={} items={items}",
            started.elapsed().as_millis()
        );
    }
}

impl Default for AssetQueryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use super::*;
    use crate::{db, models::NewAsset};

    struct TestDb {
        _dir: tempfile::TempDir,
        path: PathBuf,
        runtime: Arc<crate::services::db_pool::DatabaseRuntime>,
    }

    fn test_db(asset_count: i64) -> TestDb {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("media.db");
        let conn = db::open_connection(&path).expect("open db");
        db::init_schema(&conn).expect("init schema");
        db::add_scan_root(&conn, "/library").expect("add scan root");
        for id in 1..=asset_count {
            let asset = NewAsset {
                path: format!("/library/photo-{id}.jpg"),
                kind: "image".to_string(),
                size_bytes: 10,
                modified_at: 100 - id,
                width: Some(10),
                height: Some(10),
                duration_ms: None,
                thumb_path: None,
            };
            db::upsert_scanned_asset(&conn, &asset, 1, "/library", 1).expect("insert asset");
        }
        drop(conn);
        TestDb {
            _dir: dir,
            runtime: crate::services::db_pool::DatabaseRuntime::new(path.clone()),
            path,
        }
    }

    fn filters(tags_and: &[&str]) -> AssetQueryFilters {
        AssetQueryFilters {
            tags_and: tags_and.iter().map(|tag| tag.to_string()).collect(),
            tags_not: vec![],
            kind: None,
            favorites_only: false,
            meta_filter: None,
        }
    }

    fn start(
        manager: &AssetQueryManager,
        path: &Path,
        query_filters: &AssetQueryFilters,
        request_id: u64,
        generation: u64,
    ) -> StartAssetQueryResult {
        manager
            .start(
                &crate::services::db_pool::DatabaseRuntime::new(path.to_path_buf())
                    .admit()
                    .unwrap(),
                query_filters.clone(),
                10,
                request_id,
                generation,
            )
            .expect("start")
    }

    #[test]
    fn equal_key_reuses_cached_session_id() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let first = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready {
            session_id, total, ..
        } = first
        else {
            panic!("expected ready result");
        };
        assert_eq!(total, 3);

        let second_request = manager.begin_request("test", 2);
        let second = start(&manager, &target.path, &filters(&[]), second_request, 2);
        let StartAssetQueryResult::Ready {
            session_id: reused_session_id,
            ..
        } = second
        else {
            panic!("expected ready result for cache hit");
        };
        assert_eq!(reused_session_id, session_id);
    }

    #[test]
    fn position_uses_full_order_and_distinguishes_missing_from_stale() {
        let target = test_db(400);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let StartAssetQueryResult::Ready {
            session_id, items, ..
        } = start(&manager, &target.path, &filters(&[]), request, 1)
        else {
            panic!("ready");
        };
        assert_eq!(items.len(), 10);
        let permit = target.runtime.admit().unwrap();
        assert_eq!(
            manager.position(&permit, session_id, 201).unwrap(),
            AssetQueryPositionResult::Resolved { index: 200 }
        );
        assert_eq!(
            manager.position(&permit, session_id, 401).unwrap(),
            AssetQueryPositionResult::Missing
        );
        let conn = permit.connection().unwrap();
        db::bump_library_revision(&conn).unwrap();
        assert_eq!(
            manager.position(&permit, session_id, 201).unwrap(),
            AssetQueryPositionResult::Stale
        );
        assert!(manager.session_by_id(session_id).is_none());
        assert_eq!(
            manager.position(&permit, u64::MAX, 201).unwrap(),
            AssetQueryPositionResult::Stale
        );
        for (result, json) in [
            (
                AssetQueryPositionResult::Resolved { index: 200 },
                serde_json::json!({"status":"resolved","index":200}),
            ),
            (
                AssetQueryPositionResult::Missing,
                serde_json::json!({"status":"missing"}),
            ),
            (
                AssetQueryPositionResult::Stale,
                serde_json::json!({"status":"stale"}),
            ),
        ] {
            assert_eq!(serde_json::to_value(result).unwrap(), json);
        }
    }

    fn decode_id_range(bytes: &[u8]) -> Vec<i64> {
        let (chunks, rest) = bytes.as_chunks::<8>();
        assert!(rest.is_empty());
        let values: Vec<f64> = chunks
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk))
            .collect();
        assert_eq!(values[0] as usize, values.len() - 1, "count header");
        values[1..].iter().map(|value| *value as i64).collect()
    }

    #[test]
    fn id_range_returns_clamped_inclusive_order_and_empty_buffer_when_stale() {
        let target = test_db(400);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let StartAssetQueryResult::Ready { session_id, .. } =
            start(&manager, &target.path, &filters(&[]), request, 1)
        else {
            panic!("ready");
        };
        let permit = target.runtime.admit().unwrap();
        let range = |start, end| {
            manager
                .id_range(&permit, session_id, start, end)
                .unwrap()
                .map(|bytes| decode_id_range(&bytes))
        };
        // Newest first: index i holds asset ID i + 1 in this fixture.
        assert_eq!(range(0, 2), Some(vec![1, 2, 3]));
        assert_eq!(range(398, 399), Some(vec![399, 400]));
        assert_eq!(range(0, 399).unwrap().len(), 400);
        assert_eq!(range(395, 10_000), Some(vec![396, 397, 398, 399, 400]));
        assert_eq!(range(5, 5), Some(vec![6]));
        // Empty but ready ranges still carry the eight-byte count header.
        assert_eq!(range(6, 5), Some(vec![]));
        assert_eq!(range(400, 410), Some(vec![]));
        assert_eq!(
            manager
                .id_range(&permit, session_id, 6, 5)
                .unwrap()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(manager.id_range(&permit, u64::MAX, 0, 1).unwrap(), None);
        let conn = permit.connection().unwrap();
        db::bump_library_revision(&conn).unwrap();
        assert_eq!(manager.id_range(&permit, session_id, 0, 1).unwrap(), None);
        assert!(manager.session_by_id(session_id).is_none());
    }

    #[test]
    fn id_range_encoding_rejects_ids_that_f64_cannot_represent() {
        assert_eq!(
            decode_id_range(&encode_id_range(&[]).unwrap()),
            Vec::<i64>::new()
        );
        assert_eq!(
            decode_id_range(&encode_id_range(&[1, MAX_EXACT_F64_ID]).unwrap()),
            vec![1, MAX_EXACT_F64_ID]
        );
        assert!(encode_id_range(&[MAX_EXACT_F64_ID + 1]).is_err());
        assert!(encode_id_range(&[-1]).is_err());
    }

    #[test]
    fn start_at_returns_the_page_containing_the_anchor_on_fresh_and_cached_sessions() {
        let target = test_db(45);
        let manager = AssetQueryManager::new();
        let permit = target.runtime.admit().unwrap();
        let mut generation = 0;
        let mut start_at = |anchor| {
            generation += 1;
            let request = manager.begin_request("test", generation);
            let StartAssetQueryResult::Ready {
                session_id,
                total,
                offset,
                items,
                ..
            } = manager
                .start_at(&permit, filters(&[]), 10, request, generation, anchor)
                .unwrap()
            else {
                panic!("ready");
            };
            assert_eq!(total, 45);
            (
                session_id,
                offset,
                items.iter().map(|item| item.id).collect::<Vec<_>>(),
            )
        };
        let (fresh, offset, ids) = start_at(Some(23));
        assert_eq!((offset, ids), (20, (21..=30).collect::<Vec<_>>()));
        // Cache hit with the same key: same session, anchored page.
        let (cached, offset, ids) = start_at(Some(40));
        assert_eq!(cached, fresh);
        assert_eq!((offset, ids), (40, (41..=45).collect::<Vec<_>>()));
        // Past the end clamps to the last row's page; no anchor is page zero.
        assert_eq!(start_at(Some(10_000)).1, 40);
        assert_eq!(start_at(None).1, 0);
        assert_eq!(start_at(Some(0)).2, (1..=10).collect::<Vec<_>>());
    }

    #[test]
    fn start_at_on_an_empty_result_returns_offset_zero() {
        let target = test_db(0);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let result = manager
            .start_at(
                &target.runtime.admit().unwrap(),
                filters(&[]),
                10,
                request,
                1,
                Some(500),
            )
            .unwrap();
        let StartAssetQueryResult::Ready {
            offset,
            items,
            total,
            ..
        } = result
        else {
            panic!("ready");
        };
        assert_eq!((offset, items.len(), total), (0, 0, 0));
    }

    #[test]
    fn older_registered_request_cannot_supersede_newer_one() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();

        // Request A registers first but its blocking work is scheduled later;
        // request B registered after it must stay authoritative.
        let request_a = manager.begin_request("test", 1);
        let request_b = manager.begin_request("test", 2);

        let newest = start(&manager, &target.path, &filters(&[]), request_b, 2);
        assert!(matches!(newest, StartAssetQueryResult::Ready { .. }));

        let oldest = start(&manager, &target.path, &filters(&[]), request_a, 1);
        assert_eq!(oldest, StartAssetQueryResult::Superseded);
    }

    #[test]
    fn inverted_admission_preserves_the_newest_filter_and_allows_a_new_owner() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        // The second IPC call is polled first, then the obsolete first call.
        let newest = manager.begin_request("first-owner", 99);
        let oldest = manager.begin_request("first-owner", 98);
        assert_eq!(
            start(&manager, &target.path, &filters(&[]), oldest, 98),
            StartAssetQueryResult::Superseded
        );
        let (session, _, total) = ready_id(start(
            &manager,
            &target.path,
            &filters(&["absent"]),
            newest,
            99,
        ));
        assert_eq!(total, 0);
        assert!(matches!(
            manager
                .page(&target.runtime.admit().unwrap(), session, 0, 1)
                .unwrap(),
            AssetQueryPageResult::Ready { .. }
        ));
        assert_eq!(
            start(&manager, &target.path, &filters(&[]), oldest, 98),
            StartAssetQueryResult::Superseded
        );
        let replacement = manager.begin_request("remounted-owner", 1);
        assert_eq!(
            ready_id(start(&manager, &target.path, &filters(&[]), replacement, 1)).2,
            3
        );
        assert_eq!(
            start(&manager, &target.path, &filters(&[]), newest, 99),
            StartAssetQueryResult::Superseded
        );
        // An old generation cannot become authoritative even after another owner queried.
        assert_eq!(manager.begin_request("first-owner", 98), 0);
    }

    #[test]
    fn obsolete_client_generation_is_superseded() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();

        let stale_request = manager.begin_request("test", 7);
        manager.begin_request("test", 8);

        let result = start(&manager, &target.path, &filters(&[]), stale_request, 7);
        assert_eq!(result, StartAssetQueryResult::Superseded);
    }

    #[test]
    fn revision_bump_makes_page_stale_and_removes_session() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let ready = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready {
            session_id,
            revision,
            ..
        } = ready
        else {
            panic!("expected ready result");
        };

        let page = manager
            .page(&target.runtime.admit().unwrap(), session_id, 0, 10)
            .expect("page before bump");
        assert!(matches!(page, AssetQueryPageResult::Ready { .. }));

        let conn = db::open_connection(&target.path).expect("reopen db");
        let bumped = db::bump_library_revision(&conn).expect("bump revision");
        assert!(bumped > revision);
        drop(conn);

        let stale = manager
            .page(&target.runtime.admit().unwrap(), session_id, 0, 10)
            .expect("stale page");
        assert_eq!(stale, AssetQueryPageResult::Stale);

        let again = manager
            .page(&target.runtime.admit().unwrap(), session_id, 0, 10)
            .expect("again");
        assert_eq!(again, AssetQueryPageResult::Stale);
    }

    #[test]
    fn lru_evicts_oldest_session_beyond_four_entries() {
        let target = test_db(5);
        let manager = AssetQueryManager::new();
        let mut first_session_id = None;
        for index in 0..5 {
            let key_filters = filters(&[&format!("tag-{index}")]);
            let request = manager.begin_request("test", index as u64 + 1);
            let ready = start(
                &manager,
                &target.path,
                &key_filters,
                request,
                index as u64 + 1,
            );
            let StartAssetQueryResult::Ready { session_id, .. } = ready else {
                panic!("expected ready result for key {index}");
            };
            if index == 0 {
                first_session_id = Some(session_id);
            }
        }

        let evicted = manager
            .page(
                &target.runtime.admit().unwrap(),
                first_session_id.expect("first session"),
                0,
                10,
            )
            .expect("evicted page");
        assert_eq!(evicted, AssetQueryPageResult::Stale);
    }

    #[test]
    fn expired_sessions_become_stale() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let ready = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready { session_id, .. } = ready else {
            panic!("expected ready result");
        };

        if let Ok(mut cache) = manager.cache.lock() {
            for session in cache.sessions.iter_mut() {
                session.last_accessed = Instant::now()
                    .checked_sub(SESSION_TTL + Duration::from_secs(1))
                    .expect("expired timestamp");
            }
        }

        let expired = manager
            .page(&target.runtime.admit().unwrap(), session_id, 0, 10)
            .expect("expired page");
        assert_eq!(expired, AssetQueryPageResult::Stale);
    }

    /// Moves every session's last access `by` into the past, as if the app
    /// had been idle that long.
    fn idle_for(manager: &AssetQueryManager, by: Duration) {
        let mut cache = manager.cache.lock().unwrap();
        for session in cache.sessions.iter_mut() {
            session.last_accessed = session
                .last_accessed
                .checked_sub(by)
                .expect("idle timestamp");
        }
    }

    #[test]
    fn one_id_keepalive_reads_keep_a_selection_session_past_the_ttl() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let ready = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready { session_id, .. } = ready else {
            panic!("expected ready result");
        };
        let spec = SelectionSpec {
            session_id,
            ranges: vec![[0, 2]],
            ..SelectionSpec::default()
        };

        // Twice the TTL in total, but never longer than a keepalive interval
        // without a read: the one-ID range read restarts the idle expiry.
        for _ in 0..10 {
            idle_for(&manager, SESSION_TTL / 5);
            let keepalive = manager
                .id_range(&target.runtime.admit().unwrap(), session_id, 0, 0)
                .expect("keepalive read")
                .expect("session kept alive");
            assert_eq!(keepalive.len(), 16, "count plus one ID");
        }
        let resolved = manager
            .resolve_selection(&spec)
            .expect("resolve")
            .expect("session still present");
        assert_eq!(resolved.asset_ids, vec![1, 2, 3]);

        // Without a read for longer than the TTL the session is gone, so a
        // selection write on it reports stale instead of writing.
        idle_for(&manager, SESSION_TTL + Duration::from_secs(1));
        assert!(manager.resolve_selection(&spec).expect("resolve").is_none());
        assert!(manager
            .id_range(&target.runtime.admit().unwrap(), session_id, 0, 0)
            .expect("expired keepalive")
            .is_none());
    }

    #[test]
    fn clear_invalidates_in_flight_requests_and_sessions() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request("test", 1);
        let ready = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready { session_id, .. } = ready else {
            panic!("expected ready result");
        };

        manager.clear();

        let stale = manager
            .page(&target.runtime.admit().unwrap(), session_id, 0, 10)
            .expect("cleared page");
        assert_eq!(stale, AssetQueryPageResult::Stale);

        // A request registered before clear() is superseded by it...
        let orphan_request = manager.begin_request("test", 9);
        manager.clear();
        let late = start(&manager, &target.path, &filters(&[]), orphan_request, 9);
        assert_eq!(late, StartAssetQueryResult::Superseded);

        // ...while a request registered after the final clear() proceeds.
        let fresh_request = manager.begin_request("replacement", 2);
        let fresh = start(&manager, &target.path, &filters(&[]), fresh_request, 2);
        assert!(matches!(fresh, StartAssetQueryResult::Ready { .. }));
    }

    #[test]
    fn snapshot_start_reports_current_revision() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let conn = db::open_connection(&target.path).expect("open db");
        let expected_revision = db::current_library_revision(&conn).expect("revision");
        drop(conn);

        let request = manager.begin_request("test", 1);
        let ready = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready {
            session_id,
            revision,
            offset,
            items,
            ..
        } = ready
        else {
            panic!("expected ready result");
        };
        assert_eq!(revision, expected_revision);
        assert_eq!(offset, 0);
        assert_eq!(items.len(), 3);
        assert_eq!(
            manager
                .page(&target.runtime.admit().unwrap(), session_id, 3, 10)
                .expect("end clamp"),
            AssetQueryPageResult::Ready {
                session_id,
                revision,
                total: 3,
                offset: 3,
                items: vec![]
            }
        );
    }

    #[test]
    fn authoritative_capacity_failure_recovers_with_a_narrower_query() {
        let target = test_db(32);
        let mut manager = AssetQueryManager::new();
        manager.max_id_bytes = 64;
        let permit = target.runtime.admit().unwrap();
        let request = manager.begin_request("test", 1);
        let error = manager
            .start(&permit, filters(&[]), 2, request, 1)
            .expect_err("authoritative oversized query must fail explicitly");
        assert!(error.to_string().contains("Narrow your search"));
        assert_eq!(manager.latest_request.load(Ordering::SeqCst), request);
        assert_eq!(manager.cache.lock().unwrap().epoch, 0);
        let mut conn = db::open_connection(&target.path).unwrap();
        db::set_asset_tags_with_revision(&mut conn, 1, &["small".into()]).unwrap();
        db::set_asset_tags_with_revision(&mut conn, 2, &["small".into()]).unwrap();
        let request = manager.begin_request("test", 2);
        let (session, _, total) = ready_id(
            manager
                .start(&permit, filters(&["small"]), 1, request, 2)
                .unwrap(),
        );
        assert_eq!(total, 2);
        let AssetQueryPageResult::Ready { items, .. } =
            manager.page(&permit, session, 1, 1).unwrap()
        else {
            panic!("page")
        };
        assert_eq!(items[0].id, 2);
        assert_eq!(
            manager.position(&permit, session, 2).unwrap(),
            AssetQueryPositionResult::Resolved { index: 1 }
        );
    }

    fn dated_asset(conn: &rusqlite::Connection, name: &str, modified_at: i64) -> i64 {
        let asset = NewAsset {
            path: format!("/library/{name}.jpg"),
            kind: "image".to_string(),
            size_bytes: 10,
            modified_at,
            width: Some(10),
            height: Some(10),
            duration_ms: None,
            thumb_path: None,
        };
        db::upsert_scanned_asset(conn, &asset, 1, "/library", 1).expect("insert asset");
        conn.query_row(
            "SELECT id FROM assets WHERE path = ?1",
            [asset.path.as_str()],
            |row| row.get(0),
        )
        .expect("asset id")
    }

    fn ready_sections(
        manager: &AssetQueryManager,
        permit: &OperationPermit,
        session_id: u64,
    ) -> (usize, Vec<(u32, usize, usize)>) {
        match manager.sections(permit, session_id).expect("sections") {
            AssetQuerySectionsResult::Ready {
                session_id: returned,
                total,
                sections,
            } => {
                assert_eq!(returned, session_id);
                (
                    total,
                    sections
                        .into_iter()
                        .map(|section| (section.t, section.start, section.count))
                        .collect(),
                )
            }
            AssetQuerySectionsResult::Stale => panic!("expected ready sections"),
        }
    }

    #[test]
    fn sections_follow_bucket_months_filters_and_go_stale_with_the_session() {
        const MAR_2024: i64 = 1_710_000_000; // 2024-03-09
        const FEB_2024: i64 = 1_707_000_000; // 2024-02-03
        const JAN_2024: i64 = 1_705_000_000; // 2024-01-11
        let target = test_db(0);
        let manager = AssetQueryManager::new();
        let permit = target.runtime.admit().unwrap();

        // Empty library: a ready, empty section list.
        let request = manager.begin_request("test", 1);
        let StartAssetQueryResult::Ready { session_id, .. } =
            start(&manager, &target.path, &filters(&[]), request, 1)
        else {
            panic!("ready");
        };
        assert_eq!(ready_sections(&manager, &permit, session_id), (0, vec![]));

        let conn = permit.connection().unwrap();
        let march = dated_asset(&conn, "march", MAR_2024);
        let grouped_feb = dated_asset(&conn, "grouped-feb", FEB_2024);
        let grouped_jan = dated_asset(&conn, "grouped-jan", JAN_2024);
        let _january = dated_asset(&conn, "january", JAN_2024 - 10);
        let undated = dated_asset(&conn, "undated", 0);
        // The January member sorts with its group in February.
        db::set_asset_media_group(&conn, grouped_feb, Some("trip"), Some(1.0)).unwrap();
        db::set_asset_media_group(&conn, grouped_jan, Some("trip"), Some(2.0)).unwrap();
        db::bump_library_revision(&conn).unwrap();
        assert_eq!(
            manager.sections(&permit, session_id).unwrap(),
            AssetQuerySectionsResult::Stale
        );

        let request = manager.begin_request("test", 2);
        let StartAssetQueryResult::Ready {
            session_id, total, ..
        } = start(&manager, &target.path, &filters(&[]), request, 2)
        else {
            panic!("ready");
        };
        assert_eq!(total, 5);
        assert_eq!(
            ready_sections(&manager, &permit, session_id),
            (
                5,
                vec![(202_403, 0, 1), (202_402, 1, 2), (202_401, 3, 1), (0, 4, 1)]
            )
        );
        let Some(bytes) = manager.id_range(&permit, session_id, 0, 4).unwrap() else {
            panic!("ready range");
        };
        let ids = decode_id_range(&bytes);
        assert_eq!(ids[0], march);
        assert_eq!(&ids[1..3], &[grouped_feb, grouped_jan]);
        assert_eq!(ids[4], undated);

        // Unfiltered sections are read lazily, but never from a changed table:
        // a first read after a revision change is stale, not recomputed.
        let request = manager.begin_request("test", 10);
        let StartAssetQueryResult::Ready {
            session_id: lazy, ..
        } = start(&manager, &target.path, &filters(&[]), request, 10)
        else {
            panic!("ready");
        };
        assert!(manager
            .session_by_id(lazy)
            .unwrap()
            .sections
            .get()
            .is_some());
        db::bump_library_revision(&conn).unwrap();
        let request = manager.begin_request("test", 11);
        let StartAssetQueryResult::Ready {
            session_id: fresh, ..
        } = start(&manager, &target.path, &filters(&[]), request, 11)
        else {
            panic!("ready");
        };
        assert!(manager
            .session_by_id(fresh)
            .unwrap()
            .sections
            .get()
            .is_none());
        dated_asset(&conn, "late", MAR_2024 + 10);
        db::bump_library_revision(&conn).unwrap();
        assert_eq!(
            manager.sections(&permit, fresh).unwrap(),
            AssetQuerySectionsResult::Stale
        );

        // With a filter the group's month comes from its matching members only.
        db::set_asset_favorite(&conn, grouped_jan, true).unwrap();
        db::set_asset_favorite(&conn, undated, true).unwrap();
        let request = manager.begin_request("test", 12);
        let favorites = AssetQueryFilters {
            favorites_only: true,
            ..filters(&[])
        };
        let StartAssetQueryResult::Ready { session_id, .. } =
            start(&manager, &target.path, &favorites, request, 12)
        else {
            panic!("ready");
        };
        assert_eq!(
            ready_sections(&manager, &permit, session_id),
            (2, vec![(202_401, 0, 1), (0, 1, 1)])
        );
        assert_eq!(
            manager.sections(&permit, u64::MAX).unwrap(),
            AssetQuerySectionsResult::Stale
        );
        assert_eq!(
            serde_json::to_value(manager.sections(&permit, session_id).unwrap()).unwrap(),
            serde_json::json!({
                "status": "ready",
                "session_id": session_id,
                "total": 2,
                "sections": [{"t": 202401, "start": 0, "count": 1}, {"t": 0, "start": 1, "count": 1}]
            })
        );
    }

    #[test]
    fn unbumped_scan_commit_keeps_the_session_without_sections_until_the_next_bump() {
        const MAR_2024: i64 = 1_710_000_000; // 2024-03-09
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let permit = target.runtime.admit().unwrap();
        let request = manager.begin_request("test", 1);
        let StartAssetQueryResult::Ready {
            session_id, total, ..
        } = start(&manager, &target.path, &filters(&[]), request, 1)
        else {
            panic!("ready");
        };
        assert_eq!(total, 3);

        // A scan batch between two throttled bumps: committed, no bump.
        let conn = permit.connection().unwrap();
        let revision = db::current_library_revision(&conn).unwrap();
        dated_asset(&conn, "batch", MAR_2024);
        assert_eq!(db::current_library_revision(&conn).unwrap(), revision);

        // The session is still the snapshot its revision promises: no
        // sections, nothing stored, and pages, positions and the cached start
        // keep using it.
        for _ in 0..2 {
            assert_eq!(ready_sections(&manager, &permit, session_id), (3, vec![]));
        }
        let session = manager.session_by_id(session_id).expect("session kept");
        assert!(session.sections.get().is_none());
        let AssetQueryPageResult::Ready {
            session_id: page_session,
            total: page_total,
            items,
            ..
        } = manager.page(&permit, session_id, 0, 10).unwrap()
        else {
            panic!("ready page");
        };
        assert_eq!((page_session, page_total, items.len()), (session_id, 3, 3));
        let request = manager.begin_request("test", 2);
        let StartAssetQueryResult::Ready {
            session_id: cached,
            total,
            ..
        } = start(&manager, &target.path, &filters(&[]), request, 2)
        else {
            panic!("ready");
        };
        assert_eq!((cached, total), (session_id, 3));

        // The scan's final bump makes the session stale; the next one reads
        // sections that cover the new row.
        db::bump_library_revision(&conn).unwrap();
        assert_eq!(
            manager.sections(&permit, session_id).unwrap(),
            AssetQuerySectionsResult::Stale
        );
        assert!(manager.session_by_id(session_id).is_none());
        let request = manager.begin_request("test", 3);
        let StartAssetQueryResult::Ready {
            session_id: fresh,
            total,
            ..
        } = start(&manager, &target.path, &filters(&[]), request, 3)
        else {
            panic!("ready");
        };
        assert_ne!(fresh, session_id);
        assert_eq!(total, 4);
        assert_eq!(
            ready_sections(&manager, &permit, fresh),
            (4, vec![(202_403, 0, 1), (197_001, 1, 3)])
        );
        assert!(manager
            .session_by_id(fresh)
            .unwrap()
            .sections
            .get()
            .is_some());
    }

    #[test]
    fn cache_budget_and_epoch_are_checked_at_insertion() {
        let mut manager = AssetQueryManager::new();
        manager.max_id_bytes = 16;
        let request = manager.begin_request("test", 1);
        let session = |id| QuerySession {
            id,
            dependency_revision: 1,
            dependencies: vec!["general".into()],
            key: id.to_string(),
            asset_ids: Arc::new(vec![1, 2]),
            sections: Arc::default(),
            last_accessed: Instant::now(),
        };
        assert!(manager.insert_session(session(1), request, 0).unwrap());
        assert!(manager.insert_session(session(2), request, 0).unwrap());
        assert!(manager.session_by_id(1).is_none());
        manager.clear();
        let fresh = manager.begin_request("replacement", 1);
        assert!(!manager.insert_session(session(3), fresh, 0).unwrap());
        assert!(manager.insert_session(session(4), fresh, 1).unwrap());
    }

    fn ready_id(result: StartAssetQueryResult) -> (u64, i64, usize) {
        let StartAssetQueryResult::Ready {
            session_id,
            revision,
            total,
            ..
        } = result
        else {
            panic!("ready")
        };
        (session_id, revision, total)
    }

    #[test]
    fn scoped_mutations_preserve_unread_pages_positions_and_cached_starts() {
        // Include, exclude, combined, count, favorite, kind and group dependencies.
        for case in 0..8 {
            let target = test_db(32);
            let mut conn = db::open_connection(&target.path).unwrap();
            for id in 1..=32 {
                db::set_asset_tags_with_revision(&mut conn, id, &["common".into(), "old".into()])
                    .unwrap();
                db::set_asset_favorite_with_revision(&mut conn, id, true).unwrap();
            }
            let mut filter = filters(&[]);
            match case {
                1 => filter.tags_and = vec!["common".into()],
                2 => filter.tags_not = vec!["excluded".into()],
                3 => {
                    filter.tags_and = vec!["common".into()];
                    filter.tags_not = vec!["excluded".into()];
                    filter.favorites_only = true;
                }
                4 => filter.meta_filter = Some(AssetMetaFilter::HasNoTags { tag_count: 2 }),
                5 => filter.favorites_only = true,
                6 => filter.kind = Some("image".into()),
                7 => {
                    filter.meta_filter = Some(AssetMetaFilter::GroupName {
                        group_name: "absent".into(),
                    })
                }
                _ => {}
            }
            let manager = AssetQueryManager::new();
            let (id, revision, total) = ready_id(start(
                &manager,
                &target.path,
                &filter,
                manager.begin_request("test", 1),
                1,
            ));
            let changed = db::set_asset_tags_with_revision(
                &mut conn,
                20,
                &["common".into(), "żółć|%_:'新".into()],
            )
            .unwrap();
            assert!(changed.revision > revision);
            let permit = target.runtime.admit().unwrap();
            let AssetQueryPageResult::Ready {
                session_id,
                revision: public,
                items,
                total: page_total,
                ..
            } = manager.page(&permit, id, 16, 16).unwrap()
            else {
                panic!("unrelated edit, case {case}")
            };
            assert_eq!(session_id, id);
            assert_eq!(public, changed.revision);
            assert_eq!(page_total, total);
            assert_eq!(
                items.iter().map(|a| a.id).collect::<Vec<_>>(),
                if total == 0 {
                    vec![]
                } else {
                    (17..=32).collect()
                }
            );
            assert_eq!(
                manager.position(&permit, id, 20).unwrap(),
                if total == 0 {
                    AssetQueryPositionResult::Missing
                } else {
                    AssetQueryPositionResult::Resolved { index: 19 }
                }
            );
            assert_eq!(
                ready_id(start(
                    &manager,
                    &target.path,
                    &filter,
                    manager.begin_request("test", 2),
                    2
                )),
                (id, changed.revision, total)
            );
            // Relevant edits stale the same snapshot.
            match case {
                1 | 3 => {
                    db::set_asset_tags_with_revision(&mut conn, 20, &["other".into()]).unwrap();
                }
                2 => {
                    db::set_asset_tags_with_revision(&mut conn, 20, &["excluded".into()]).unwrap();
                }
                4 => {
                    db::set_asset_tags_with_revision(&mut conn, 20, &[]).unwrap();
                }
                5 => {
                    db::set_asset_favorite_with_revision(&mut conn, 20, false).unwrap();
                }
                _ => {
                    db::bump_library_revision(&conn).unwrap();
                }
            }
            assert_eq!(
                manager.page(&permit, id, 16, 16).unwrap(),
                AssetQueryPageResult::Stale
            );
        }
    }

    #[test]
    fn favorites_noops_and_missing_ids_preserve_dependencies_but_report_global_bumps() {
        let target = test_db(32);
        let mut conn = db::open_connection(&target.path).unwrap();
        let manager = AssetQueryManager::new();
        let mut filter = filters(&[]);
        filter.favorites_only = true;
        let (id, initial, _) = ready_id(start(
            &manager,
            &target.path,
            &filter,
            manager.begin_request("test", 1),
            1,
        ));
        db::set_asset_favorite_with_revision(&mut conn, 20, false).unwrap();
        let revision = db::set_asset_favorite_with_revision(&mut conn, 999, true).unwrap();
        assert_eq!(revision, initial + 2);
        assert_eq!(
            ready_id(start(
                &manager,
                &target.path,
                &filter,
                manager.begin_request("test", 2),
                2
            )),
            (id, revision, 0)
        );
        let all = filters(&[]);
        let (all_id, _, _) = ready_id(start(
            &manager,
            &target.path,
            &all,
            manager.begin_request("test", 3),
            3,
        ));
        let bulk = db::toggle_assets_favorite_bulk(&mut conn, &[20, 999]).unwrap();
        let permit = target.runtime.admit().unwrap();
        assert_eq!(
            manager.page(&permit, id, 0, 10).unwrap(),
            AssetQueryPageResult::Stale
        );
        let AssetQueryPageResult::Ready {
            items, revision, ..
        } = manager.page(&permit, all_id, 19, 1).unwrap()
        else {
            panic!("ready")
        };
        assert!(items[0].is_favorite);
        assert_eq!(revision, bulk.revision);
    }

    #[test]
    fn removed_tag_markers_survive_orphan_cleanup_and_recreation() {
        let target = test_db(20);
        let mut conn = db::open_connection(&target.path).unwrap();
        let manager = AssetQueryManager::new();
        let filter = filters(&["żółć|%_:'新"]);
        let (empty, _, _) = ready_id(start(
            &manager,
            &target.path,
            &filter,
            manager.begin_request("test", 1),
            1,
        ));
        db::set_asset_tags_with_revision(&mut conn, 20, &filter.tags_and).unwrap();
        assert_eq!(
            manager
                .position(&target.runtime.admit().unwrap(), empty, 20)
                .unwrap(),
            AssetQueryPositionResult::Stale
        );
        let (present, _, _) = ready_id(start(
            &manager,
            &target.path,
            &filter,
            manager.begin_request("test", 2),
            2,
        ));
        let removed = db::set_asset_tags_with_revision(&mut conn, 20, &[]).unwrap();
        assert_eq!(
            db::query_dependency_revision(&conn, &filter.dependencies()).unwrap(),
            removed.revision
        );
        assert_eq!(
            manager
                .position(&target.runtime.admit().unwrap(), present, 20)
                .unwrap(),
            AssetQueryPositionResult::Stale
        );
        let (empty_again, _, _) = ready_id(start(
            &manager,
            &target.path,
            &filter,
            manager.begin_request("test", 3),
            3,
        ));
        db::set_asset_tags_with_revision(&mut conn, 20, &filter.tags_and).unwrap();
        assert_eq!(
            manager
                .position(&target.runtime.admit().unwrap(), empty_again, 20)
                .unwrap(),
            AssetQueryPositionResult::Stale
        );
    }

    #[test]
    fn wal_reads_keep_revision_dependencies_and_rows_in_one_snapshot() {
        use std::sync::Barrier;
        for mode in ["fresh", "cached", "page", "position"] {
            let target = test_db(32);
            let mut conn = db::open_connection(&target.path).unwrap();
            db::toggle_assets_favorite_bulk(&mut conn, &(1..=32).collect::<Vec<_>>()).unwrap();
            let before = db::current_library_revision(&conn).unwrap();
            let manager = AssetQueryManager::new();
            let mut filter = filters(&[]);
            filter.favorites_only = true;
            let initial = if mode == "fresh" {
                None
            } else {
                Some(
                    ready_id(start(
                        &manager,
                        &target.path,
                        &filter,
                        manager.begin_request("test", 1),
                        1,
                    ))
                    .0,
                )
            };
            let barrier = Arc::new(Barrier::new(2));
            let hook_barrier = barrier.clone();
            *manager.after_revision_read.lock().unwrap() = Some(Box::new(move || {
                hook_barrier.wait();
                hook_barrier.wait();
            }));
            std::thread::scope(|scope| {
                let read = scope.spawn(|| {
                    let permit = target.runtime.admit().unwrap();
                    match mode {
                        "fresh" | "cached" => {
                            let result = manager
                                .start(
                                    &permit,
                                    filter.clone(),
                                    32,
                                    manager.begin_request("test", 2),
                                    2,
                                )
                                .unwrap();
                            let StartAssetQueryResult::Ready {
                                session_id,
                                revision,
                                items,
                                total,
                                ..
                            } = result
                            else {
                                panic!("ready")
                            };
                            assert_eq!(revision, before);
                            assert_eq!(total, 32);
                            assert!(items.iter().all(|a| a.is_favorite));
                            if let Some(id) = initial {
                                assert_eq!(id, session_id);
                            }
                            session_id
                        }
                        "page" => {
                            let id = initial.unwrap();
                            let AssetQueryPageResult::Ready {
                                revision, items, ..
                            } = manager.page(&permit, id, 16, 16).unwrap()
                            else {
                                panic!("ready")
                            };
                            assert_eq!(revision, before);
                            assert!(items.iter().all(|a| a.is_favorite));
                            id
                        }
                        _ => {
                            let id = initial.unwrap();
                            assert_eq!(
                                manager.position(&permit, id, 20).unwrap(),
                                AssetQueryPositionResult::Resolved { index: 19 }
                            );
                            id
                        }
                    }
                });
                barrier.wait();
                db::set_asset_favorite_with_revision(&mut conn, 20, false).unwrap();
                barrier.wait();
                let id = read.join().unwrap();
                let permit = target.runtime.admit().unwrap();
                assert_eq!(
                    manager.page(&permit, id, 16, 16).unwrap(),
                    AssetQueryPageResult::Stale
                );
                let (_, revision, total) = ready_id(start(
                    &manager,
                    &target.path,
                    &filter,
                    manager.begin_request("test", 3),
                    3,
                ));
                assert_eq!(revision, before + 1);
                assert_eq!(total, 31);
            });
        }
    }
}
