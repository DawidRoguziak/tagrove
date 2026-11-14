//! Native video, all in the Tauri adapter (ADR 0034, D1): the libmpv player,
//! its native event client, the GTK/OpenGL surface above the WebView and the
//! command input types it draws with. The core keeps only the asset-ID source
//! authorization (`services::video_source_service`).

mod mpv_events;
pub mod player;
pub mod surface;
pub mod types;
