pub mod app;
pub mod commands;
pub mod db;
pub mod error;
pub mod indexer;
pub mod models;
pub mod services;
pub mod thumbs;
pub mod utils;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Mutex, RwLock},
};

use tauri::Manager;

use app::{instance_lock::InstanceLock, state::AppState};
use commands::{
    assets::{
        apply_duplicate_resolution_batch, delete_asset, find_duplicate_assets, get_asset_details,
        get_asset_query_page, get_video_stream_url, list_assets, list_tags, merge_asset_tags_bulk,
        rename_asset_file, set_asset_favorite,
        set_asset_media_group, set_asset_tags, set_assets_media_group_bulk, start_asset_query,
    },
    import_export::{
        clear_library_data, export_db_bundle, export_tags_csv, import_db_bundle, import_tags_csv,
        inspect_db_bundle,
    },
    scan::{add_scan_root, list_scan_roots, remove_scan_root, rescan_all_roots, scan_folder},
    thumbs::{
        cancel_render_all_thumbnails, clear_all_thumbnails, ensure_asset_thumbnail,
        ensure_page_thumbnails, ensure_thumbnails, get_video_tool_status, render_all_thumbnails,
        render_failed_thumbnails,
    },
    window::sync_window_theme,
};
use services::{
    asset_mutation_service, media_server::MediaServerState, thumb_scheduler::ThumbnailScheduler,
};

#[cfg(debug_assertions)]
const PRODUCTION_APP_IDENTIFIER: &str = "com.example.mediatagger";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            if app.config().identifier == PRODUCTION_APP_IDENTIFIER {
                return Err(format!(
                    "Refusing to start a debug build with production identifier {PRODUCTION_APP_IDENTIFIER}. Use tauri.conf.dev.json or tauri.conf.e2e.json."
                )
                .into());
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("cannot resolve app_data_dir: {e}"))?;
            fs::create_dir_all(&app_data_dir)?;
            let instance_lock = InstanceLock::acquire(&app_data_dir)?;
            app.manage(instance_lock);

            let thumbs_dir = app_data_dir.join("thumbs");
            fs::create_dir_all(&thumbs_dir)?;

            let db_path = app_data_dir.join("media.db");
            let conn = db::open_connection(&db_path)?;
            db::init_schema(&conn)?;
            asset_mutation_service::recover_pending_file_operations(&conn)?;
            let media_server = MediaServerState::start(db_path.clone())?;
            app.manage(media_server);

            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|e| format!("cannot resolve resource_dir: {e}"))?;
            let ffmpeg_path = resolve_ffmpeg_path(&resource_dir);
            let thumb_scheduler =
                ThumbnailScheduler::new(resolve_thumbnail_worker_count(), ffmpeg_path.clone());

            app.manage(AppState {
                db_path,
                thumbs_dir,
                ffmpeg_path,
                scan_lock: Mutex::new(()),
                thumb_lock: RwLock::new(()),
                thumb_scheduler,
                thumbnail_render_all_running: AtomicBool::new(false),
                thumbnail_render_all_cancel_requested: AtomicBool::new(false),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            list_scan_roots,
            add_scan_root,
            remove_scan_root,
            rescan_all_roots,
            ensure_asset_thumbnail,
            ensure_page_thumbnails,
            ensure_thumbnails,
            get_video_tool_status,
            render_all_thumbnails,
            render_failed_thumbnails,
            cancel_render_all_thumbnails,
            clear_all_thumbnails,
            clear_library_data,
            list_assets,
            start_asset_query,
            get_asset_query_page,
            get_asset_details,
            get_video_stream_url,
            set_asset_tags,
            merge_asset_tags_bulk,
            set_asset_favorite,
            set_asset_media_group,
            set_assets_media_group_bulk,
            delete_asset,
            find_duplicate_assets,
            rename_asset_file,
            apply_duplicate_resolution_batch,
            list_tags,
            export_tags_csv,
            import_tags_csv,
            export_db_bundle,
            inspect_db_bundle,
            import_db_bundle,
            sync_window_theme
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}

fn resolve_ffmpeg_path(resource_dir: &Path) -> PathBuf {
    let mut candidates = Vec::new();
    for directory in collect_ffmpeg_candidate_dirs(resource_dir) {
        extend_ffmpeg_candidates(&mut candidates, &directory);
    }

    #[cfg(not(windows))]
    candidates.push(PathBuf::from("/usr/bin/ffmpeg"));

    for candidate in candidates {
        if is_executable_file(&candidate) {
            return candidate;
        }
    }

    PathBuf::from("ffmpeg")
}

fn collect_ffmpeg_candidate_dirs(resource_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![resource_dir.to_path_buf(), resource_dir.join("binaries")];

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            dirs.push(exe_dir.to_path_buf());
            dirs.push(exe_dir.join("binaries"));
            dirs.push(exe_dir.join("resources"));
            dirs.push(exe_dir.join("resources").join("binaries"));

            if let Some(exe_parent) = exe_dir.parent() {
                dirs.push(exe_parent.join("resources"));
                dirs.push(exe_parent.join("resources").join("binaries"));
            }
        }
    }

    let mut seen = HashSet::new();
    dirs.into_iter()
        .filter(|directory| seen.insert(directory.clone()))
        .collect()
}

fn extend_ffmpeg_candidates(candidates: &mut Vec<PathBuf>, directory: &Path) {
    #[cfg(windows)]
    candidates.push(directory.join("ffmpeg.exe"));
    candidates.push(directory.join("ffmpeg"));
    #[cfg(windows)]
    if let Some(sidecar) = resolve_sidecar_ffmpeg(directory) {
        candidates.push(sidecar);
    }
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    true
}

#[cfg(windows)]
fn resolve_sidecar_ffmpeg(directory: &Path) -> Option<PathBuf> {
    let mut sidecar_candidates = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            if !path.is_file() {
                return false;
            }

            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.starts_with("ffmpeg-"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    sidecar_candidates.sort_unstable();
    sidecar_candidates.into_iter().next()
}

fn resolve_thumbnail_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get().saturating_sub(2).clamp(2, 8))
        .unwrap_or(4)
}
