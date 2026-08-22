use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use crate::{
    db::{self, AssetMetaFilter},
    error::AppResult,
    models::{AssetQueryPageResult, StartAssetQueryResult},
    services::db_pool,
};

const MAX_SESSIONS: usize = 4;
const SESSION_TTL: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct AssetQueryFilters {
    pub tags_and: Vec<String>,
    pub tags_not: Vec<String>,
    pub kind: Option<String>,
    pub favorites_only: bool,
    pub meta_filter: Option<AssetMetaFilter>,
}

#[derive(Clone)]
struct QuerySession {
    id: u64,
    revision: i64,
    key: String,
    asset_ids: Arc<Vec<i64>>,
    last_accessed: Instant,
}

#[derive(Default)]
struct QueryCache {
    sessions: VecDeque<QuerySession>,
}

pub struct AssetQueryManager {
    next_session_id: AtomicU64,
    latest_request: AtomicU64,
    latest_generation: AtomicU64,
    cache: Mutex<QueryCache>,
}

impl AssetQueryManager {
    fn new() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            latest_request: AtomicU64::new(0),
            latest_generation: AtomicU64::new(0),
            cache: Mutex::new(QueryCache::default()),
        }
    }

    /// Registers a query request in arrival order before any blocking work is
    /// scheduled. The returned token identifies this request; a later
    /// registration supersedes it.
    pub fn begin_request(&self, generation: u64) -> u64 {
        let request_id = self.latest_request.fetch_add(1, Ordering::SeqCst) + 1;
        self.latest_generation.store(generation, Ordering::SeqCst);
        request_id
    }

    fn is_superseded(&self, request_id: u64, generation: u64) -> bool {
        self.latest_request.load(Ordering::SeqCst) != request_id
            || self.latest_generation.load(Ordering::SeqCst) != generation
    }

    pub fn start(
        &self,
        db_path: &Path,
        filters: AssetQueryFilters,
        page_size: usize,
        request_id: u64,
        generation: u64,
    ) -> AppResult<StartAssetQueryResult> {
        let conn = db_pool::connection(db_path)?;
        // One deferred read transaction gives revision, ordered IDs, and the
        // first page a single consistent SQLite snapshot.
        let tx = conn.unchecked_transaction()?;
        let revision = db::current_library_revision(&tx)?;
        let key = filter_key(&filters, revision);
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
        let Some(asset_ids) = db::try_list_ordered_asset_ids_with_meta(
            &tx,
            &filters.tags_and,
            &filters.tags_not,
            filters.kind.as_deref(),
            filters.favorites_only,
            filters.meta_filter.as_ref(),
            cancel_check,
        )? else {
            perf_log("asset-query-cancelled", started, 0);
            return Ok(StartAssetQueryResult::Superseded);
        };
        perf_log("asset-query-build", started, asset_ids.len());

        if self.is_superseded(request_id, generation) {
            return Ok(StartAssetQueryResult::Superseded);
        }

        let session = QuerySession {
            id: self.next_session_id.fetch_add(1, Ordering::Relaxed),
            revision,
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
        self.insert_session(session);

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
        db_path: &Path,
        session_id: u64,
        offset: usize,
        limit: usize,
    ) -> AppResult<AssetQueryPageResult> {
        let conn = db_pool::connection(db_path)?;
        let tx = conn.unchecked_transaction()?;
        let revision = db::current_library_revision(&tx)?;
        let Some(session) = self.session_by_id(session_id) else {
            return Ok(AssetQueryPageResult::Stale);
        };
        if session.revision != revision {
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

    fn cached_session(&self, key: &str) -> Option<QuerySession> {
        let mut cache = self.cache.lock().ok()?;
        prune(&mut cache);
        let index = cache.sessions.iter().position(|session| session.key == key)?;
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

    fn insert_session(&self, session: QuerySession) {
        if let Ok(mut cache) = self.cache.lock() {
            prune(&mut cache);
            cache.sessions.retain(|existing| existing.key != session.key);
            cache.sessions.push_front(session);
            cache.sessions.truncate(MAX_SESSIONS);
        }
    }

    fn remove_session(&self, id: u64) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.sessions.retain(|session| session.id != id);
        }
    }

    pub fn clear(&self) {
        self.latest_request.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut cache) = self.cache.lock() {
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

pub fn manager() -> &'static AssetQueryManager {
    static MANAGER: OnceLock<AssetQueryManager> = OnceLock::new();
    MANAGER.get_or_init(AssetQueryManager::new)
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
            db::upsert_scanned_asset(&conn, &asset, 1, "/library", 1)
                .expect("insert asset");
        }
        drop(conn);
        TestDb { _dir: dir, path }
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
            .start(path, query_filters.clone(), 10, request_id, generation)
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
            session_id, revision, ..
        } = ready
        else {
            panic!("expected ready result");
        };

        let page = manager
            .page(&target.path, session_id, 0, 10)
            .expect("page before bump");
        assert!(matches!(page, AssetQueryPageResult::Ready { .. }));

        let conn = db::open_connection(&target.path).expect("reopen db");
        let bumped = db::bump_library_revision(&conn).expect("bump revision");
        assert!(bumped > revision);
        drop(conn);

        let stale = manager.page(&target.path, session_id, 0, 10).expect("stale page");
        assert_eq!(stale, AssetQueryPageResult::Stale);

        let again = manager.page(&target.path, session_id, 0, 10).expect("again");
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
            let ready = start(&manager, &target.path, &key_filters, request, index as u64 + 1);
            let StartAssetQueryResult::Ready { session_id, .. } = ready else {
                panic!("expected ready result for key {index}");
            };
            if index == 0 {
                first_session_id = Some(session_id);
            }
        }

        let evicted = manager
            .page(&target.path, first_session_id.expect("first session"), 0, 10)
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

        let expired = manager.page(&target.path, session_id, 0, 10).expect("expired page");
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

        let stale = manager.page(&target.path, session_id, 0, 10).expect("cleared page");
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
            manager.page(&target.path, session_id, 3, 10).expect("end clamp"),
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
    fn service_manager_singleton_is_process_wide() {
        assert!(std::ptr::eq(manager(), manager()));
    }
}
