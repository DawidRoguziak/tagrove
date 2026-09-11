use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use crate::{
    db::{self, AssetMetaFilter},
    error::AppResult,
    models::{AssetQueryPageResult, AssetQueryPositionResult, StartAssetQueryResult},
    services::db_pool::OperationPermit,
};

const MAX_SESSIONS: usize = 4;
const MAX_RETAINED_ID_BYTES: usize = 64 * 1024 * 1024;
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);

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
            max_id_bytes: MAX_RETAINED_ID_BYTES,
            #[cfg(test)]
            after_revision_read: Mutex::new(None),
        }
    }

    /// Registers a query request in arrival order before any blocking work is
    /// scheduled. The returned token identifies this request; a later
    /// registration supersedes it.
    pub fn begin_request(&self, _generation: u64) -> u64 {
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
        let result = self.start_inner(permit, filters, page_size, request_id, generation);
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
            let items = db::list_asset_summaries_by_ids(
                &tx,
                &session.asset_ids[..session.asset_ids.len().min(page_size)],
            )?;
            return Ok(StartAssetQueryResult::Ready {
                session_id: session.id,
                revision,
                total: session.asset_ids.len(),
                offset: 0,
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
        let Some(asset_ids) = ids_result? else {
            perf_log("asset-query-cancelled", started, 0);
            return Ok(StartAssetQueryResult::Superseded);
        };
        perf_log("asset-query-build", started, asset_ids.len());

        if self.is_superseded(request_id, generation) {
            return Ok(StartAssetQueryResult::Superseded);
        }

        let session = QuerySession {
            id: self.next_session_id.fetch_add(1, Ordering::Relaxed),
            dependency_revision,
            dependencies,
            key,
            asset_ids: Arc::new(asset_ids),
            last_accessed: Instant::now(),
        };
        let items = db::list_asset_summaries_by_ids(
            &tx,
            &session.asset_ids[..session.asset_ids.len().min(page_size)],
        )?;
        let total = session.asset_ids.len();
        let session_id = session.id;
        drop(tx);
        if !self.insert_session(session, request_id, epoch) {
            return Ok(StartAssetQueryResult::Superseded);
        }

        Ok(StartAssetQueryResult::Ready {
            session_id,
            revision,
            total,
            offset: 0,
            items,
        })
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

    fn insert_session(&self, session: QuerySession, request_id: u64, epoch: u64) -> bool {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.epoch != epoch || self.latest_request.load(Ordering::SeqCst) != request_id {
                return false;
            }
            if session.asset_ids.capacity() * std::mem::size_of::<i64>() > self.max_id_bytes {
                return false;
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
            return true;
        }
        false
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
        let request = manager.begin_request(1);
        let first = start(&manager, &target.path, &filters(&[]), request, 1);
        let StartAssetQueryResult::Ready {
            session_id, total, ..
        } = first
        else {
            panic!("expected ready result");
        };
        assert_eq!(total, 3);

        let second_request = manager.begin_request(2);
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
        let request = manager.begin_request(1);
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

    #[test]
    fn older_registered_request_cannot_supersede_newer_one() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();

        // Request A registers first but its blocking work is scheduled later;
        // request B registered after it must stay authoritative.
        let request_a = manager.begin_request(1);
        let request_b = manager.begin_request(2);

        let newest = start(&manager, &target.path, &filters(&[]), request_b, 2);
        assert!(matches!(newest, StartAssetQueryResult::Ready { .. }));

        let oldest = start(&manager, &target.path, &filters(&[]), request_a, 1);
        assert_eq!(oldest, StartAssetQueryResult::Superseded);
    }

    #[test]
    fn obsolete_client_generation_is_superseded() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();

        let stale_request = manager.begin_request(7);
        manager.begin_request(8);

        let result = start(&manager, &target.path, &filters(&[]), stale_request, 7);
        assert_eq!(result, StartAssetQueryResult::Superseded);
    }

    #[test]
    fn revision_bump_makes_page_stale_and_removes_session() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request(1);
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
            let request = manager.begin_request(index as u64 + 1);
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
        let request = manager.begin_request(1);
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

    #[test]
    fn clear_invalidates_in_flight_requests_and_sessions() {
        let target = test_db(3);
        let manager = AssetQueryManager::new();
        let request = manager.begin_request(1);
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
        let orphan_request = manager.begin_request(9);
        manager.clear();
        let late = start(&manager, &target.path, &filters(&[]), orphan_request, 9);
        assert_eq!(late, StartAssetQueryResult::Superseded);

        // ...while a request registered after the final clear() proceeds.
        let fresh_request = manager.begin_request(2);
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

        let request = manager.begin_request(1);
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
    fn cache_budget_and_epoch_are_checked_at_insertion() {
        let mut manager = AssetQueryManager::new();
        manager.max_id_bytes = 16;
        let request = manager.begin_request(1);
        let session = |id| QuerySession {
            id,
            dependency_revision: 1,
            dependencies: vec!["general".into()],
            key: id.to_string(),
            asset_ids: Arc::new(vec![1, 2]),
            last_accessed: Instant::now(),
        };
        assert!(manager.insert_session(session(1), request, 0));
        assert!(manager.insert_session(session(2), request, 0));
        assert!(manager.session_by_id(1).is_none());
        manager.clear();
        let fresh = manager.begin_request(1);
        assert!(!manager.insert_session(session(3), fresh, 0));
        assert!(manager.insert_session(session(4), fresh, 1));
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
                manager.begin_request(1),
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
                    manager.begin_request(2),
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
            manager.begin_request(1),
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
                manager.begin_request(2),
                2
            )),
            (id, revision, 0)
        );
        let all = filters(&[]);
        let (all_id, _, _) = ready_id(start(
            &manager,
            &target.path,
            &all,
            manager.begin_request(3),
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
            manager.begin_request(1),
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
            manager.begin_request(2),
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
            manager.begin_request(3),
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
                        manager.begin_request(1),
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
                                .start(&permit, filter.clone(), 32, manager.begin_request(2), 2)
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
                    manager.begin_request(3),
                    3,
                ));
                assert_eq!(revision, before + 1);
                assert_eq!(total, 31);
            });
        }
    }
}
