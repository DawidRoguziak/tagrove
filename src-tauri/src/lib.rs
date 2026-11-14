pub mod adapter;
pub mod app;
pub mod commands;
pub mod db;
pub mod error;
pub mod indexer;
pub mod models;
pub mod services;
#[cfg(test)]
pub(crate) mod test_support;
pub mod thumbs;
pub mod usecases;
pub mod utils;
pub mod video;

use std::fs;

use tauri::Manager;

use app::instance_lock::InstanceLock;
use commands::video::{
    begin_video_open, cancel_video_open, close_video, control_video, open_video, set_video_bounds,
    set_video_control_labels,
};
use commands::{
    assets::{
        apply_duplicate_resolution_batch, apply_selection_mutation, delete_asset,
        find_duplicate_assets, get_asset_details, get_asset_query_id_range, get_asset_query_page,
        get_asset_query_position, get_asset_query_sections, get_asset_summaries_by_ids,
        get_startup_popular_tags, list_tags, merge_asset_tags_bulk, set_asset_favorite,
        set_asset_media_group, set_asset_tags, set_assets_media_group_bulk, start_asset_query,
        toggle_assets_favorite_bulk,
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
        cancel_render_all_thumbnails, clear_all_thumbnails, get_video_tool_status,
        render_all_thumbnails, render_failed_thumbnails,
    },
    window::sync_window_theme,
};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::Builder::new().open_js_links_on_click(false).build())
        // `thumb://localhost/<id>?v=<version>`: numeric IDs only, see
        // adapter/thumb_scheme.rs. Requests before setup finishes get 503.
        .register_asynchronous_uri_scheme_protocol(
            services::thumb_protocol::SCHEME,
            |ctx, request, responder| {
                adapter::thumb_scheme::handle(ctx.app_handle(), request, responder)
            },
        )
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

            let (storage, snapshots) = app::bootstrap::open_storage(&app_data_dir)?;
            app.manage(snapshots.popular_tags);
            app.manage(adapter::startup_scan::StartupScanState::new(snapshots.scan_roots));

            let resource_dir = app
                .path()
                .resource_dir()
                .map_err(|e| format!("cannot resolve resource_dir: {e}"))?;
            let ffmpeg_path = app::bootstrap::resolve_ffmpeg_path(&resource_dir);
            // GTK activates the environment locale during Tauri initialization, so
            // libmpv's required numeric locale must be restored after GTK starts.
            let locale = unsafe { libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr()) };
            if locale.is_null() {
                return Err("failed to activate the C numeric locale".into());
            }
            let player = video::player::VideoPlayerService::new();
            let main_window = app
                .get_webview_window("main")
                .ok_or("main WebView window is unavailable")?;
            video::surface::setup(&main_window, &player)?;
            app.manage(player);
            // Last step: `thumb://` answers 503 until `AppState` is managed.
            app.manage(app::bootstrap::start_workers(&storage, ffmpeg_path)?);

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
            get_video_tool_status,
            render_all_thumbnails,
            render_failed_thumbnails,
            cancel_render_all_thumbnails,
            clear_all_thumbnails,
            clear_library_data,
            start_asset_query,
            get_asset_query_page,
            get_asset_query_id_range,
            get_asset_query_sections,
            get_asset_query_position,
            get_asset_details, get_asset_summaries_by_ids,
            set_asset_tags,
            merge_asset_tags_bulk,
            set_asset_favorite,
            toggle_assets_favorite_bulk,
            apply_selection_mutation,
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
