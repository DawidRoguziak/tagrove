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
    cache: Mutex<QueryCache>,
}

impl AssetQueryManager {
    fn new() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            latest_request: AtomicU64::new(0),
            cache: Mutex::new(QueryCache::default()),
        }
    }

    pub fn start(
        &self,
        db_path: &Path,
        filters: AssetQueryFilters,
        page_size: usize,
    ) -> AppResult<StartAssetQueryResult> {
        let request_id = self.latest_request.fetch_add(1, Ordering::SeqCst) + 1;
        let conn = db_pool::connection(db_path)?;
        let revision = db::current_library_revision(&conn)?;
        let key = filter_key(&filters, revision);
        let page_size = page_size.clamp(1, 256);

        if let Some(session) = self.cached_session(&key) {
            let items = db::list_asset_summaries_by_ids(
                &conn,
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

        let started = Instant::now();
        let asset_ids = db::list_ordered_asset_ids_with_meta(
            &conn,
            &filters.tags_and,
            &filters.tags_not,
            filters.kind.as_deref(),
            filters.favorites_only,
            filters.meta_filter.as_ref(),
        )?;
        perf_log("asset-query-build", started, asset_ids.len());

        if self.latest_request.load(Ordering::SeqCst) != request_id {
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
            &conn,
            &session.asset_ids[..session.asset_ids.len().min(page_size)],
        )?;
        let total = session.asset_ids.len();
        let session_id = session.id;
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
        let revision = db::current_library_revision(&conn)?;
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
        let items = db::list_asset_summaries_by_ids(&conn, &session.asset_ids[start..end])?;
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
