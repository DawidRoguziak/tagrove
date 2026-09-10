pub mod app;
pub mod commands;
pub mod db;
pub mod error;
pub mod indexer;
pub mod models;
pub mod services;
pub mod thumbs;
pub mod utils;
pub mod video_surface;

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, atomic::AtomicU64, Mutex, RwLock},
};

use tauri::Manager;

use app::{instance_lock::InstanceLock, state::AppState};
use commands::video::{
    begin_video_open, cancel_video_open, close_video, control_video, open_video, set_video_bounds,
    set_video_control_labels,
};
use commands::{
    assets::{
        apply_duplicate_resolution_batch, delete_asset, find_duplicate_assets, get_asset_details,
        get_asset_query_page, get_asset_query_position, get_asset_summaries_by_ids,
        get_startup_popular_tags, list_assets, list_tags, merge_asset_tags_bulk,
        set_asset_favorite, set_asset_media_group, set_asset_tags, set_assets_media_group_bulk,
        start_asset_query, toggle_assets_favorite_bulk,
    },
    import_export::{
        clear_library_data, export_db_bundle, export_tags_csv, import_db_bundle, import_tags_csv,
        inspect_db_bundle,
    },
    scan::{
        add_scan_root, list_scan_roots, remove_scan_root, rescan_all_roots, scan_folder,
        scan_startup_roots, set_scan_root_auto_scan,
    },
    thumbs::{
        cancel_render_all_thumbnails, clear_all_thumbnails, ensure_thumbnails,
        get_video_tool_status, render_all_thumbnails, render_failed_thumbnails,
    },
    window::sync_window_theme,
};
use services::{asset_mutation_service, thumb_scheduler::ThumbnailScheduler};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            if !app::profile::is_debug_profile(&app.config().identifier) {
                return Err(format!(
                    "Refusing to start a debug build with identifier {}. Use tauri.conf.dev.json or tauri.conf.e2e.json.",
                    app.config().identifier
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

            services::backup_service::recover_interrupted_restore(&app_data_dir)?;

            let thumbs_dir = app_data_dir.join("thumbs");
            fs::create_dir_all(&thumbs_dir)?;

            let db_path = app_data_dir.join("media.db");
            let conn = db::open_connection(&db_path)?;
            db::init_schema(&conn)?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            asset_mutation_service::recover_pending_file_operations(&conn)?;
            let popular_tags = db::list_popular_tags(&conn).unwrap_or_else(|error| {
                eprintln!("Failed to calculate startup popular tags: {error}");
                Vec::new()
            });
            app.manage(app::state::StartupPopularTags::new(popular_tags));
            app.manage(app::state::StartupScanState::new(db::list_scan_root_settings(&conn)?));

            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|e| format!("cannot resolve resource_dir: {e}"))?;
            let ffmpeg_path = resolve_ffmpeg_path(&resource_dir);
            // GTK activates the environment locale during Tauri initialization, so
            // libmpv's required numeric locale must be restored after GTK starts.
            let locale = unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
            if locale.is_null() {
                return Err("failed to activate the C numeric locale".into());
            }
            let player = services::video_player_service::VideoPlayerService::new();
            let main_window = app
                .get_webview_window("main")
                .ok_or("main WebView window is unavailable")?;
            video_surface::setup(&main_window, &player)?;
            app.manage(player);
            let thumb_scheduler =
                ThumbnailScheduler::from_available_parallelism(ffmpeg_path.clone());
            // Demand calls block on scheduler results; a scheduler whose
            // workers never started would hang every thumbnail command.
            // Refuse the startup instead of entering a wedged state.
            let worker_spawn_failures = thumb_scheduler.worker_spawn_failures();
            if worker_spawn_failures > 0 {
                return Err(format!(
                    "failed to start {worker_spawn_failures} thumbnail worker thread(s); refusing to run with an unhealthy thumbnail scheduler"
                )
                .into());
            }

            app.manage(AppState {
                database: crate::services::db_pool::DatabaseRuntime::new(db_path.clone()),
                db_path,
                thumbs_dir,
                ffmpeg_path,
                scan_lock: Mutex::new(()),
                thumb_lock: RwLock::new(()),
                thumb_scheduler,
                thumbnail_render_all_running: AtomicBool::new(false),
                thumbnail_render_all_cancel_requested: AtomicBool::new(false),
                thumbnail_generation: AtomicU64::new(0),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            set_scan_root_auto_scan,
            scan_startup_roots,
            list_scan_roots,
            add_scan_root,
            remove_scan_root,
            rescan_all_roots,
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
            get_asset_query_position,
            get_asset_details, get_asset_summaries_by_ids,
            set_asset_tags,
            merge_asset_tags_bulk,
            set_asset_favorite,
            toggle_assets_favorite_bulk,
            set_asset_media_group,
            set_assets_media_group_bulk,
            delete_asset,
            find_duplicate_assets,
            apply_duplicate_resolution_batch,
            list_tags,
            get_startup_popular_tags,
            export_tags_csv,
            import_tags_csv,
            export_db_bundle,
            inspect_db_bundle,
            import_db_bundle,
            sync_window_theme,
            begin_video_open,
            cancel_video_open,
            set_video_control_labels,
            open_video,
            set_video_bounds,
            control_video,
            close_video
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}

fn resolve_ffmpeg_path(resource_dir: &Path) -> PathBuf {
    let mut candidates = Vec::new();
    for directory in collect_ffmpeg_candidate_dirs(resource_dir) {
        extend_ffmpeg_candidates(&mut candidates, &directory);
    }

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
    candidates.push(directory.join("ffmpeg"));
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}
