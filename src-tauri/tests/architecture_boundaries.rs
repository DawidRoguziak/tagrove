//! Architecture guard of the hexagonal-backend plan (ADR 0034) and a frozen
//! snapshot of the IPC wire contract.
//!
//! `import_rules` keeps Tauri, GTK, WebKit and libmpv out of the backend core
//! and freezes the import direction. Its allowlists may only shrink: an entry
//! that no longer breaks its rule fails `allowlist_entries_are_still_needed`.
//! Only the plan's orchestrator edits them, after a stage is accepted
//! (`docs/plans/hexagonal-backend/README.md`).
//!
//! `wire_snapshot` freezes the registered commands, their wire arguments, the
//! JSON shape of the response and input DTOs and the emitted event names. An
//! intended IPC change updates this snapshot in the same change
//! (`docs/architecture/ipc-contract.md`, safe contract-change checklist).

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

// ---------------------------------------------------------------------------
// Source scanner (std only)
// ---------------------------------------------------------------------------

/// What `strip` keeps of string literals.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Literals {
    /// Blank string and char literals: only code tokens remain.
    Blank,
    /// Keep string literals verbatim (comments are still removed).
    Keep,
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Removes comments (line, nested block and doc comments) and, with
/// `Literals::Blank`, the contents of string, byte-string, raw-string and char
/// literals. Every removed character becomes a space and newlines are kept, so
/// line numbers stay valid. Lifetimes (`'a`, `'_`, `'static`) are code.
fn strip(src: &str, literals: Literals) -> String {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(src.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < n {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        let prev_is_ident = i > 0 && is_ident_char(chars[i - 1]);

        if c == '/' && next == Some('/') {
            while i < n && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            let mut depth = 0usize;
            while i < n {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
            continue;
        }

        // Raw strings: r"…", r#"…"#, br"…", br#"…"#.
        if !prev_is_ident && (c == 'r' || (c == 'b' && next == Some('r'))) {
            let mut j = if c == 'b' { i + 2 } else { i + 1 };
            let mut hashes = 0;
            while j < n && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < n && chars[j] == '"' {
                let open_end = j + 1;
                let mut k = open_end;
                while k < n {
                    if chars[k] == '"' && (0..hashes).all(|h| chars.get(k + 1 + h) == Some(&'#')) {
                        break;
                    }
                    k += 1;
                }
                let close_end = (k + 1 + hashes).min(n);
                for (index, &ch) in chars.iter().enumerate().take(close_end).skip(i) {
                    let keep = literals == Literals::Keep || index < open_end || index >= k;
                    out.push(if keep { ch } else { blank(ch) });
                }
                i = close_end;
                continue;
            }
        }

        // Strings and byte strings with escapes.
        if c == '"' || (c == 'b' && next == Some('"') && !prev_is_ident) {
            let open_end = if c == 'b' { i + 2 } else { i + 1 };
            let mut k = open_end;
            while k < n && chars[k] != '"' {
                k += if chars[k] == '\\' { 2 } else { 1 };
            }
            let close_end = (k + 1).min(n);
            for (index, &ch) in chars.iter().enumerate().take(close_end).skip(i) {
                let keep = literals == Literals::Keep || index < open_end || index >= k;
                out.push(if keep { ch } else { blank(ch) });
            }
            i = close_end;
            continue;
        }

        // Char literals ('x', '\n', '\u{..}', '"'); anything else is a lifetime.
        if c == '\'' {
            let end = if next == Some('\\') {
                let mut k = i + 2;
                while k < n && chars[k] != '\'' {
                    k += 1;
                }
                Some(k)
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(end) = end {
                let close_end = (end + 1).min(n);
                for &ch in &chars[i..close_end] {
                    out.push(if literals == Literals::Keep {
                        ch
                    } else {
                        blank(ch)
                    });
                }
                i = close_end;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    line: usize,
    text: String,
}

impl Token {
    fn is(&self, text: &str) -> bool {
        self.text == text
    }

    fn is_ident(&self) -> bool {
        self.text.chars().next().is_some_and(is_ident_start)
    }
}

/// Splits stripped code into identifiers, `::` and single punctuation chars.
fn tokens(code: &str) -> Vec<Token> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if is_ident_char(c) {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            out.push(Token {
                line,
                text: chars[start..i].iter().collect(),
            });
        } else if c == ':' && chars.get(i + 1) == Some(&':') {
            out.push(Token {
                line,
                text: "::".into(),
            });
            i += 2;
        } else {
            out.push(Token {
                line,
                text: c.to_string(),
            });
            i += 1;
        }
    }
    out
}

/// Top-level crate modules a file refers to: `crate::X`, the members of
/// `crate::{X::…, Y::{…}}`, and the same after `super::super::` (the crate
/// root as seen from `db/*.rs`).
fn crate_root_refs(toks: &[Token]) -> Vec<(usize, String)> {
    let mut refs = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let after = if toks[i].is("crate") && toks.get(i + 1).is_some_and(|t| t.is("::")) {
            Some(i + 2)
        } else if toks[i].is("super")
            && toks.get(i + 1).is_some_and(|t| t.is("::"))
            && toks.get(i + 2).is_some_and(|t| t.is("super"))
            && toks.get(i + 3).is_some_and(|t| t.is("::"))
        {
            Some(i + 4)
        } else {
            None
        };
        let Some(start) = after else {
            i += 1;
            continue;
        };
        match toks.get(start) {
            Some(t) if t.is("{") => {
                let mut depth = 0usize;
                let mut expect_segment = true;
                let mut k = start;
                while k < toks.len() {
                    let t = &toks[k];
                    if t.is("{") {
                        depth += 1;
                        expect_segment = depth == 1;
                    } else if t.is("}") {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if depth == 1 && t.is(",") {
                        expect_segment = true;
                    } else if depth == 1 && expect_segment && t.is_ident() {
                        refs.push((t.line, t.text.clone()));
                        expect_segment = false;
                    } else {
                        expect_segment = false;
                    }
                    k += 1;
                }
                i = k;
            }
            Some(t) if t.is_ident() => {
                refs.push((t.line, t.text.clone()));
                i = start + 1;
            }
            _ => i = start,
        }
    }
    refs
}

/// Source roots of the backend. Stage 08 adds the core crate's `src`.
fn source_roots() -> Vec<PathBuf> {
    vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")]
}

struct SourceFile {
    /// Path relative to its source root, with `/` separators.
    rel: String,
    text: String,
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<SourceFile>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            out.push(SourceFile { rel, text });
        }
    }
}

fn source_files() -> Vec<SourceFile> {
    let mut files = Vec::new();
    for root in source_roots() {
        collect_rs(&root, &root, &mut files);
    }
    assert!(
        files.iter().any(|f| f.rel == "lib.rs"),
        "source scan found no lib.rs"
    );
    files
}

fn read_source(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn scanner_ignores_comments_and_literals() {
    let src = r##"
// tauri in a comment
/* gtk /* nested glib */ libmpv2 */
/// doc: use tauri::AppHandle;
const ORIGIN: &str = "tauri://localhost";
const RAW: &str = r#"gtk "quoted" glib"#;
const BYTES: &[u8] = b"webkit2gtk";
const QUOTE: char = '"';
const ESCAPED: char = '\'';
fn f<'a>(x: &'a str) -> &'a str { x }
let my_tauri_x = 1;
"##;
    let code = strip(src, Literals::Blank);
    let hits = framework_hits(&tokens(&code));
    assert!(hits.is_empty(), "unexpected hits: {hits:?}");
    assert_eq!(code.lines().count(), src.lines().count());
}

#[test]
fn scanner_finds_framework_identifiers() {
    let src = "use tauri::{AppHandle, Manager};\nfn f(c: async_channel::Sender<()>) {}\n\
               fn g() { tauri_plugin_dialog::init(); }";
    let hits = framework_hits(&tokens(&strip(src, Literals::Blank)));
    let names: Vec<_> = hits.iter().map(|(line, t)| format!("{line}:{t}")).collect();
    assert_eq!(
        names,
        [
            "1:tauri",
            "1:AppHandle",
            "2:async_channel",
            "3:tauri_plugin_dialog"
        ]
    );
}

#[test]
fn scanner_expands_crate_groups() {
    let src = "use crate::{\n    app::state::AppState,\n    commands::video::{VideoBounds, VideoControlLabels},\n    db,\n};\n\
               fn f() { crate::services::x(); super::super::indexer::y(); super::thumbs::z(); }";
    let refs = crate_root_refs(&tokens(&strip(src, Literals::Blank)));
    assert_eq!(
        refs,
        [
            (2, "app".to_string()),
            (3, "commands".to_string()),
            (4, "db".to_string()),
            (6, "services".to_string()),
            (6, "indexer".to_string()),
        ]
    );
}

#[test]
fn scanner_keeps_strings_on_request() {
    let src = "app.emit(\"process-progress\", p); // \"not-an-event\"";
    assert_eq!(emitted_events(src), ["process-progress"]);
}

// ---------------------------------------------------------------------------
// Import rules
// ---------------------------------------------------------------------------

/// Identifiers of the GUI/runtime stack that only the Tauri adapter may name.
const FRAMEWORK_IDENTS: &[&str] = &[
    "tauri",
    "AppHandle",
    "gtk",
    "gdk",
    "glib",
    "webkit2gtk",
    "libmpv2",
    "libmpv2_sys",
    "async_channel",
    "libloading",
];

fn is_framework_ident(text: &str) -> bool {
    FRAMEWORK_IDENTS.contains(&text) || text.starts_with("tauri_plugin_")
}

fn framework_hits(toks: &[Token]) -> Vec<(usize, String)> {
    toks.iter()
        .filter(|t| is_framework_ident(&t.text))
        .map(|t| (t.line, t.text.clone()))
        .collect()
}

/// The permanent adapter: files and directories (trailing `/`) that may use
/// Tauri, GTK, WebKit and libmpv.
const ADAPTER_FIXED: &[&str] = &[
    "lib.rs",
    "main.rs",
    "commands/",
    "adapter/",
    "video/",
    "app/profile.rs",
];

/// Core files that still use the framework, with the plan stage that removes
/// the entry. May only shrink; empty since stage 04, so no core file may be
/// added back.
const FRAMEWORK_ALLOWLIST: &[(&str, &str)] = &[];

/// Core files that still import an adapter module (rule 3). May only shrink;
/// empty since stage 04.
const ADAPTER_IMPORT_ALLOWLIST: &[(&str, &str)] = &[];

/// Top-level modules `db` must not reach (rule 2).
const DB_FORBIDDEN_MODULES: &[&str] = &[
    "services", "app", "commands", "usecases", "adapter", "video",
];

/// Adapter modules the core must not reach (rule 3).
const ADAPTER_MODULES: &[&str] = &["commands", "adapter", "video"];

fn is_adapter(rel: &str) -> bool {
    ADAPTER_FIXED.iter().any(|entry| {
        if entry.ends_with('/') {
            rel.starts_with(entry)
        } else {
            rel == *entry
        }
    })
}

fn is_db(rel: &str) -> bool {
    rel == "db.rs" || rel.starts_with("db/")
}

fn allowed(list: &[(&str, &str)], rel: &str) -> bool {
    list.iter().any(|(path, _)| *path == rel)
}

fn rule1_violations(file: &SourceFile) -> Vec<String> {
    framework_hits(&tokens(&strip(&file.text, Literals::Blank)))
        .into_iter()
        .map(|(line, ident)| format!("src/{}:{line}: rule 1: `{ident}`", file.rel))
        .collect()
}

fn rule2_violations(file: &SourceFile) -> Vec<String> {
    crate_root_refs(&tokens(&strip(&file.text, Literals::Blank)))
        .into_iter()
        .filter(|(_, module)| DB_FORBIDDEN_MODULES.contains(&module.as_str()))
        .map(|(line, module)| format!("src/{}:{line}: rule 2: `crate::{module}`", file.rel))
        .collect()
}

fn rule3_violations(file: &SourceFile) -> Vec<String> {
    crate_root_refs(&tokens(&strip(&file.text, Literals::Blank)))
        .into_iter()
        .filter(|(_, module)| ADAPTER_MODULES.contains(&module.as_str()))
        .map(|(line, module)| format!("src/{}:{line}: rule 3: `crate::{module}`", file.rel))
        .collect()
}

fn report(rule: &str, hint: &str, violations: &[String]) {
    assert!(
        violations.is_empty(),
        "{rule}\n{}\n\n{hint} Allowlists are changed only by the plan orchestrator \
         (docs/plans/hexagonal-backend/README.md, ADR 0034).",
        violations.join("\n")
    );
}

/// Rule 1: Tauri, GTK, WebKit and libmpv only in the adapter.
#[test]
fn core_does_not_use_the_gui_runtime() {
    let violations: Vec<String> = source_files()
        .iter()
        .filter(|f| !is_adapter(&f.rel) && !allowed(FRAMEWORK_ALLOWLIST, &f.rel))
        .flat_map(rule1_violations)
        .collect();
    report(
        "Core files name the GUI runtime:",
        "Move this code to the Tauri adapter (lib.rs, commands/, adapter/, video/).",
        &violations,
    );
}

/// Rule 2: `db` does not depend on services, app state or the adapter.
#[test]
fn db_does_not_import_upper_layers() {
    let violations: Vec<String> = source_files()
        .iter()
        .filter(|f| is_db(&f.rel))
        .flat_map(rule2_violations)
        .collect();
    report(
        "The db layer imports an upper layer:",
        "db.rs and db/* may use only models, error, utils, thumbs and db itself.",
        &violations,
    );
}

/// Rule 3: no core file imports an adapter module.
#[test]
fn core_does_not_import_the_adapter() {
    let violations: Vec<String> = source_files()
        .iter()
        .filter(|f| !is_adapter(&f.rel) && !allowed(ADAPTER_IMPORT_ALLOWLIST, &f.rel))
        .flat_map(rule3_violations)
        .collect();
    report(
        "Core files import the adapter:",
        "Move the shared type into the core, or the importing code into the adapter.",
        &violations,
    );
}

/// Rule 3, adapter side: the native video stack in `video/` does not depend on
/// the IPC command layer. Commands use `video::{player, surface, types}`, never
/// the other way round.
#[test]
fn video_does_not_import_commands() {
    let violations: Vec<String> = source_files()
        .iter()
        .filter(|f| f.rel.starts_with("video/"))
        .flat_map(|file| {
            crate_root_refs(&tokens(&strip(&file.text, Literals::Blank)))
                .into_iter()
                .filter(|(_, module)| module == "commands")
                .map(|(line, module)| format!("src/{}:{line}: rule 3: `crate::{module}`", file.rel))
        })
        .collect();
    report(
        "The video adapter imports the command layer:",
        "Move the shared type into video/types.rs and import it from commands/.",
        &violations,
    );
}

/// Rule 4: command handlers are thin and reach SQLite and the workflow locks
/// only through use cases (`usecases/`). Flags a `db::` path, a `with_*lock`
/// helper, `Connection`, and any `locks` path, which every helper of
/// `app/locks.rs` needs (`try_with_scan_and_thumb_lock`, `with_thumb_operation`,
/// `with_database_maintenance`, ...).
#[test]
fn commands_do_not_touch_db_or_locks() {
    let mut violations = Vec::new();
    for file in source_files()
        .iter()
        .filter(|f| f.rel.starts_with("commands/"))
    {
        let toks = tokens(&strip(&file.text, Literals::Blank));
        for (index, t) in toks.iter().enumerate() {
            let db_path = t.is("db") && toks.get(index + 1).is_some_and(|n| n.is("::"));
            let lock_helper = t.text.starts_with("with_") && t.text.ends_with("lock");
            if db_path || lock_helper || t.is("Connection") || t.is("locks") {
                violations.push(format!("src/{}:{}: rule 4: `{}`", file.rel, t.line, t.text));
            }
        }
    }
    report(
        "Command handlers reach the database or the locks directly:",
        "Move the body into a use case in usecases/.",
        &violations,
    );
}

#[test]
fn allowlist_entries_exist() {
    let files: BTreeSet<String> = source_files().into_iter().map(|f| f.rel).collect();
    for (path, stage) in FRAMEWORK_ALLOWLIST.iter().chain(ADAPTER_IMPORT_ALLOWLIST) {
        assert!(
            files.contains(*path),
            "allowlist entry `{path}` (stage {stage}) names a missing file: remove it"
        );
        assert!(
            !is_adapter(path),
            "allowlist entry `{path}` is already in the permanent adapter set: remove it"
        );
    }
}

/// The allowlists only shrink: an entry whose file is clean must be removed.
#[test]
fn allowlist_entries_are_still_needed() {
    let files = source_files();
    let find = |rel: &str| files.iter().find(|f| f.rel == rel);
    for (path, stage) in FRAMEWORK_ALLOWLIST {
        if let Some(file) = find(path) {
            assert!(
                !rule1_violations(file).is_empty(),
                "`{path}` no longer uses the GUI runtime: remove it from FRAMEWORK_ALLOWLIST \
                 (stage {stage} done)"
            );
        }
    }
    for (path, stage) in ADAPTER_IMPORT_ALLOWLIST {
        if let Some(file) = find(path) {
            assert!(
                !rule3_violations(file).is_empty(),
                "`{path}` no longer imports the adapter: remove it from ADAPTER_IMPORT_ALLOWLIST \
                 (stage {stage} done)"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Wire snapshot
// ---------------------------------------------------------------------------

/// Commands registered in `generate_handler!`, sorted.
const REGISTERED_COMMANDS: &[&str] = &[
    "add_scan_root",
    "apply_duplicate_resolution_batch",
    "apply_selection_mutation",
    "begin_video_open",
    "cancel_render_all_thumbnails",
    "cancel_video_open",
    "clear_all_thumbnails",
    "clear_library_data",
    "close_video",
    "control_video",
    "delete_asset",
    "export_db_bundle",
    "export_tags_csv",
    "find_duplicate_assets",
    "get_asset_details",
    "get_asset_query_id_range",
    "get_asset_query_page",
    "get_asset_query_position",
    "get_asset_query_sections",
    "get_asset_summaries_by_ids",
    "get_startup_popular_tags",
    "get_video_tool_status",
    "import_db_bundle",
    "import_tags_csv",
    "inspect_db_bundle",
    "list_scan_roots",
    "list_tags",
    "merge_asset_tags_bulk",
    "open_video",
    "remove_scan_root",
    "render_all_thumbnails",
    "render_failed_thumbnails",
    "rescan_all_roots",
    "scan_folder",
    "scan_startup_roots",
    "set_asset_favorite",
    "set_asset_media_group",
    "set_asset_tags",
    "set_assets_media_group_bulk",
    "set_scan_root_auto_scan",
    "set_video_bounds",
    "set_video_control_labels",
    "start_asset_query",
    "sync_window_theme",
    "toggle_assets_favorite_bulk",
];

/// `#[tauri::command]` functions that are deliberately not registered.
const UNREGISTERED_COMMANDS: &[&str] = &["rename_asset_file"];

/// Wire signature of every registered command: arguments without the values
/// Tauri injects (state, app handle, window), path qualifiers dropped, then the
/// return type. Tauri sends each argument under its camelCase name.
const COMMAND_SIGNATURES: &[(&str, &str)] = &[
    ("add_scan_root", "(path: String) -> Result<(), String>"),
    ("apply_duplicate_resolution_batch", "(input: DuplicateResolutionBatchInput) -> Result<DuplicateResolutionBatchSummary, String>"),
    ("apply_selection_mutation", "(selection: SelectionSpec, mutation: SelectionMutation) -> Result<SelectionMutationResult, String>"),
    ("begin_video_open", "() -> u64"),
    ("cancel_render_all_thumbnails", "() -> Result<bool, String>"),
    ("cancel_video_open", "(request_id: u64) -> Result<(), String>"),
    ("clear_all_thumbnails", "() -> Result<usize, String>"),
    ("clear_library_data", "() -> Result<ClearLibrarySummary, String>"),
    ("close_video", "(session_id: u64) -> Result<(), String>"),
    ("control_video", "(session_id: u64, command: VideoControl) -> Result<(), String>"),
    ("delete_asset", "(asset_id: i64) -> Result<DeleteAssetSummary, String>"),
    ("export_db_bundle", "(path: String) -> Result<DbBundleExportSummary, String>"),
    ("export_tags_csv", "(path: String) -> Result<CsvExportSummary, String>"),
    ("find_duplicate_assets", "() -> Result<DuplicateScanSummary, String>"),
    ("get_asset_details", "(asset_id: i64) -> Result<Option<AssetDetails>, String>"),
    ("get_asset_query_id_range", "(session_id: u64, start: usize, end: usize) -> Result<Response, String>"),
    ("get_asset_query_page", "(session_id: u64, offset: usize, limit: usize) -> Result<AssetQueryPageResult, String>"),
    ("get_asset_query_position", "(session_id: u64, asset_id: i64) -> Result<AssetQueryPositionResult, String>"),
    ("get_asset_query_sections", "(session_id: u64) -> Result<AssetQuerySectionsResult, String>"),
    ("get_asset_summaries_by_ids", "(asset_ids: Vec<i64>) -> Result<Vec<AssetSummary>, String>"),
    ("get_startup_popular_tags", "() -> Vec<String>"),
    ("get_video_tool_status", "() -> Result<VideoToolStatus, String>"),
    ("import_db_bundle", "(path: String, root_mappings: Vec<DbRootMapping>) -> Result<DbBundleImportSummary, String>"),
    ("import_tags_csv", "(path: String) -> Result<CsvImportSummary, String>"),
    ("inspect_db_bundle", "(path: String) -> Result<DbBundleInspection, String>"),
    ("list_scan_roots", "() -> Result<Vec<ScanRoot>, String>"),
    ("list_tags", "(query: String, offset: i64, limit: i64) -> Result<TagListPage, String>"),
    ("merge_asset_tags_bulk", "(asset_ids: Vec<i64>, tags: Vec<String>) -> Result<BulkTagMergeSummary, String>"),
    ("open_video", "(asset_id: i64, request_id: u64, bounds: VideoBounds, control_labels: VideoControlLabels, on_event: Channel<VideoEvent>) -> Result<u64, String>"),
    ("remove_scan_root", "(path: String) -> Result<RemoveRootSummary, String>"),
    ("render_all_thumbnails", "() -> Result<ThumbnailRenderSummary, String>"),
    ("render_failed_thumbnails", "() -> Result<ThumbnailRenderSummary, String>"),
    ("rescan_all_roots", "() -> Result<ScanSummary, String>"),
    ("scan_folder", "(path: String) -> Result<ScanSummary, String>"),
    ("scan_startup_roots", "() -> Result<Option<ScanSummary>, String>"),
    ("set_asset_favorite", "(asset_id: i64, is_favorite: bool) -> Result<(), String>"),
    ("set_asset_media_group", "(asset_id: i64, media_group_key: Option<String>, media_group_order: Option<f64>) -> Result<(), String>"),
    ("set_asset_tags", "(asset_id: i64, tags: Vec<String>) -> Result<SetAssetTagsSummary, String>"),
    ("set_assets_media_group_bulk", "(updates: Vec<BulkMediaGroupUpdateInput>, media_group_key: Option<String>) -> Result<BulkMediaGroupSummary, String>"),
    ("set_scan_root_auto_scan", "(path: String, enabled: bool) -> Result<(), String>"),
    ("set_video_bounds", "(session_id: u64, bounds: VideoBounds) -> Result<(), String>"),
    ("set_video_control_labels", "(session_id: u64, control_labels: VideoControlLabels) -> Result<(), String>"),
    ("start_asset_query", "(tags_and: Vec<String>, tags_not: Vec<String>, kind: Option<String>, favorites_only: bool, meta_filter: Option<AssetMetaFilterInput>, owner_id: String, generation: u64, page_size: usize, anchor_offset: Option<usize>) -> Result<StartAssetQueryResult, String>"),
    ("sync_window_theme", "(theme: String) -> Result<(), String>"),
    ("toggle_assets_favorite_bulk", "(asset_ids: Vec<i64>) -> Result<BulkFavoriteSummary, String>"),
];

/// Names in `generate_handler![…]` of `src/lib.rs`.
fn registered_commands() -> Vec<String> {
    let toks = tokens(&strip(&read_source("lib.rs"), Literals::Blank));
    let start = toks
        .windows(3)
        .position(|w| w[0].is("generate_handler") && w[1].is("!") && w[2].is("["))
        .expect("generate_handler![ in lib.rs");
    let mut names = Vec::new();
    for t in &toks[start + 3..] {
        if t.is("]") {
            break;
        }
        if t.is_ident() {
            names.push(t.text.clone());
        }
    }
    names
}

/// Renders type tokens: path qualifiers dropped (`tauri::ipc::Response` ->
/// `Response`), a space between adjacent identifiers and after commas.
fn render_type(toks: &[Token]) -> String {
    let mut out = String::new();
    let mut previous_ident = false;
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.is_ident() && toks.get(i + 1).is_some_and(|n| n.is("::")) {
            i += 2;
            continue;
        }
        if t.is_ident() && previous_ident {
            out.push(' ');
        }
        out.push_str(&t.text);
        if t.is(",") {
            out.push(' ');
        }
        previous_ident = t.is_ident();
        i += 1;
    }
    out
}

/// Splits tokens at commas outside `<>`, `()` and `[]`.
fn split_top_level(toks: &[Token]) -> Vec<&[Token]> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (index, t) in toks.iter().enumerate() {
        match t.text.as_str() {
            "<" | "(" | "[" => depth += 1,
            ">" | ")" | "]" => depth -= 1,
            "," if depth == 0 => {
                parts.push(&toks[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < toks.len() {
        parts.push(&toks[start..]);
    }
    parts
}

const INJECTED_TYPES: &[&str] = &["State", "AppHandle", "WebviewWindow", "Window", "Webview"];

/// `name -> "(arg: Type, …) -> Return"` of every `#[tauri::command]` in
/// `src/commands/**`.
fn command_signatures() -> Vec<(String, String)> {
    let mut signatures = Vec::new();
    for file in source_files()
        .iter()
        .filter(|f| f.rel.starts_with("commands/"))
    {
        let toks = tokens(&strip(&file.text, Literals::Blank));
        let mut i = 0;
        while i + 5 < toks.len() {
            let is_command = toks[i].is("#")
                && toks[i + 1].is("[")
                && toks[i + 2].is("tauri")
                && toks[i + 3].is("::")
                && toks[i + 4].is("command");
            if !is_command {
                i += 1;
                continue;
            }
            let fn_at = (i..toks.len())
                .find(|&k| toks[k].is("fn"))
                .expect("fn after #[tauri::command]");
            let name = toks[fn_at + 1].text.clone();
            let open = (fn_at..toks.len())
                .find(|&k| toks[k].is("("))
                .expect("parameter list");
            let mut depth = 0i32;
            let mut close = open;
            for (k, t) in toks.iter().enumerate().skip(open) {
                match t.text.as_str() {
                    "(" => depth += 1,
                    ")" => {
                        depth -= 1;
                        if depth == 0 {
                            close = k;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let mut args = Vec::new();
            for param in split_top_level(&toks[open + 1..close]) {
                let colon = param
                    .iter()
                    .position(|t| t.is(":"))
                    .unwrap_or_else(|| panic!("typed parameter in {name}"));
                let ty = &param[colon + 1..];
                if ty.iter().any(|t| INJECTED_TYPES.contains(&t.text.as_str())) {
                    continue;
                }
                let arg = param[..colon]
                    .iter()
                    .rev()
                    .find(|t| t.is_ident())
                    .expect("parameter name");
                args.push(format!("{}: {}", arg.text, render_type(ty)));
            }
            let body = (close..toks.len())
                .find(|&k| toks[k].is("{") || toks[k].is("where"))
                .expect("function body");
            let returns = if toks[close + 1].is("-") && toks[close + 2].is(">") {
                render_type(&toks[close + 3..body])
            } else {
                "()".to_string()
            };
            signatures.push((name, format!("({}) -> {returns}", args.join(", "))));
            i = body;
        }
    }
    signatures.sort();
    signatures
}

/// Event names of `.emit("…")` and `.emit_to(…, "…")` calls in one file.
fn emitted_events(src: &str) -> Vec<String> {
    let code = strip(src, Literals::Keep);
    let mut names = Vec::new();
    for marker in [".emit(", ".emit_to("] {
        let mut rest = code.as_str();
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let call = &rest[..rest.find(')').unwrap_or(rest.len())];
            if let Some(open) = call.find('"') {
                let tail = &call[open + 1..];
                if let Some(close) = tail.find('"') {
                    names.push(tail[..close].to_string());
                }
            }
        }
    }
    names
}

#[test]
fn registered_commands_are_frozen() {
    let mut registered = registered_commands();
    let count = registered.len();
    registered.sort();
    registered.dedup();
    assert_eq!(
        count,
        registered.len(),
        "duplicate command in generate_handler!"
    );
    assert_eq!(
        registered, REGISTERED_COMMANDS,
        "generate_handler! changed: update REGISTERED_COMMANDS together with the Rust command, \
         the src/platform/tauri wrapper, its contracts/ type and docs/architecture/ipc-contract.md"
    );
}

#[test]
fn registered_commands_are_documented() {
    let doc_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("docs/architecture/ipc-contract.md");
    let doc = fs::read_to_string(&doc_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", doc_path.display()));
    let missing: Vec<String> = registered_commands()
        .into_iter()
        .filter(|name| !doc.contains(&format!("`{name}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "commands missing from docs/architecture/ipc-contract.md: {missing:?}"
    );
}

#[test]
fn command_signatures_are_frozen() {
    let signatures = command_signatures();
    let registered: BTreeSet<String> = registered_commands().into_iter().collect();
    let unregistered: Vec<&str> = signatures
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| !registered.contains(*name))
        .collect();
    assert_eq!(
        unregistered, UNREGISTERED_COMMANDS,
        "the set of unregistered #[tauri::command] functions changed"
    );
    let actual: Vec<(&str, &str)> = signatures
        .iter()
        .filter(|(name, _)| registered.contains(name))
        .map(|(name, signature)| (name.as_str(), signature.as_str()))
        .collect();
    let actual_set: BTreeSet<(&str, &str)> = actual.iter().copied().collect();
    let frozen_set: BTreeSet<(&str, &str)> = COMMAND_SIGNATURES.iter().copied().collect();
    let changes: Vec<String> = frozen_set
        .difference(&actual_set)
        .map(|(name, signature)| format!("  - {name}{signature}"))
        .chain(
            actual_set
                .difference(&frozen_set)
                .map(|(name, signature)| format!("  + {name}{signature}")),
        )
        .collect();
    let rendered: String = actual
        .iter()
        .map(|(name, signature)| format!("    (\"{name}\", \"{signature}\"),\n"))
        .collect();
    assert!(
        changes.is_empty(),
        "command wire signatures changed (argument names are the camelCase IPC keys):\n{}\n\n\
         Current table for COMMAND_SIGNATURES:\n{rendered}",
        changes.join("\n")
    );
}

#[test]
fn emitted_event_names_are_frozen() {
    let names: BTreeSet<String> = source_files()
        .iter()
        .flat_map(|f| emitted_events(&f.text))
        .collect();
    assert_eq!(
        names.iter().map(String::as_str).collect::<Vec<_>>(),
        ["process-progress"],
        "the set of emitted application events changed"
    );
}

mod dto_snapshot {
    //! JSON shape of the IPC DTOs. Responses are snake_case (serde defaults)
    //! with the tags and renames below; inputs are what the frontend sends.

    use media_tagger::{
        commands::{
            assets::{AssetMetaFilterInput, BulkMediaGroupUpdateInput},
            video::VideoControl,
        },
        models::*,
        services::{asset_query_service::SelectionSpec, selection_service::SelectionMutation},
        video::{
            player::{PlaybackSnapshot, VideoEvent, VideoEventPayload},
            types::{VideoBounds, VideoControlLabels},
        },
    };
    use serde_json::{json, Value};

    fn to_json<T: serde::Serialize>(value: T) -> Value {
        serde_json::to_value(value).expect("serialize")
    }

    fn from_json<T: serde::de::DeserializeOwned>(value: Value) -> T {
        serde_json::from_value(value).expect("deserialize")
    }

    fn summary() -> AssetSummary {
        AssetSummary {
            id: 7,
            file_name: "a.jpg".into(),
            preview_path: None,
            kind: "image".into(),
            modified_at: 1_700_000_000,
            width: Some(640),
            height: Some(480),
            duration_ms: None,
            thumb_version: "0123456789abcdef".into(),
            is_favorite: true,
            media_group_key: Some("g".into()),
            media_group_order: Some(1.5),
        }
    }

    fn summary_json() -> Value {
        json!({
            "id": 7,
            "file_name": "a.jpg",
            "preview_path": null,
            "kind": "image",
            "modified_at": 1_700_000_000,
            "width": 640,
            "height": 480,
            "duration_ms": null,
            "thumb_version": "0123456789abcdef",
            "is_favorite": true,
            "media_group_key": "g",
            "media_group_order": 1.5
        })
    }

    #[test]
    fn asset_reads() {
        assert_eq!(to_json(summary()), summary_json());

        let mut details = summary_json();
        let extra = json!({
            "path": "/media/a.jpg",
            "size_bytes": 1234,
            "thumb_path": "/thumbs/7.jpg",
            "tags": ["cat"]
        });
        details
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            to_json(AssetDetails {
                summary: summary(),
                path: "/media/a.jpg".into(),
                size_bytes: 1234,
                thumb_path: Some("/thumbs/7.jpg".into()),
                tags: vec!["cat".into()],
            }),
            details
        );

        assert_eq!(
            to_json(ScanRoot {
                path: "/media".into(),
                auto_scan_on_startup: true,
            }),
            json!({ "path": "/media", "auto_scan_on_startup": true })
        );
        assert_eq!(
            to_json(TagListPage {
                items: vec!["cat".into()],
                total: 1,
            }),
            json!({ "items": ["cat"], "total": 1 })
        );
    }

    #[test]
    fn query_session_results() {
        assert_eq!(
            to_json(StartAssetQueryResult::Ready {
                session_id: 3,
                revision: 9,
                total: 1,
                offset: 0,
                items: vec![summary()],
            }),
            json!({
                "status": "ready",
                "session_id": 3,
                "revision": 9,
                "total": 1,
                "offset": 0,
                "items": [summary_json()]
            })
        );
        assert_eq!(
            to_json(StartAssetQueryResult::Superseded),
            json!({ "status": "superseded" })
        );
        assert_eq!(
            to_json(AssetQueryPageResult::Ready {
                session_id: 3,
                revision: 9,
                total: 1,
                offset: 0,
                items: vec![],
            }),
            json!({
                "status": "ready",
                "session_id": 3,
                "revision": 9,
                "total": 1,
                "offset": 0,
                "items": []
            })
        );
        assert_eq!(
            to_json(AssetQueryPageResult::Stale),
            json!({ "status": "stale" })
        );
        assert_eq!(
            to_json(AssetQuerySectionsResult::Ready {
                session_id: 3,
                total: 5,
                sections: vec![AssetQuerySection {
                    t: 202_611,
                    start: 0,
                    count: 5,
                }],
            }),
            json!({
                "status": "ready",
                "session_id": 3,
                "total": 5,
                "sections": [{ "t": 202_611, "start": 0, "count": 5 }]
            })
        );
        assert_eq!(
            to_json(AssetQuerySectionsResult::Stale),
            json!({ "status": "stale" })
        );
        assert_eq!(
            to_json(AssetQueryPositionResult::Resolved { index: 4 }),
            json!({ "status": "resolved", "index": 4 })
        );
        assert_eq!(
            to_json(AssetQueryPositionResult::Missing),
            json!({ "status": "missing" })
        );
        assert_eq!(
            to_json(AssetQueryPositionResult::Stale),
            json!({ "status": "stale" })
        );
    }

    #[test]
    fn tag_and_selection_mutations() {
        assert_eq!(to_json(TagQueryImpact::None), json!({ "type": "none" }));
        assert_eq!(to_json(TagQueryImpact::All), json!({ "type": "all" }));
        let tags_impact = || TagQueryImpact::Tags {
            changed_tags: vec!["cat".into()],
            tag_count_changed: true,
        };
        let tags_impact_json =
            json!({ "type": "tags", "changed_tags": ["cat"], "tag_count_changed": true });
        assert_eq!(to_json(tags_impact()), tags_impact_json);

        assert_eq!(
            to_json(SetAssetTagsSummary {
                vocabulary_changed: true,
                query_impact: tags_impact(),
                asset_id: 7,
                changed: true,
                tags: vec!["cat".into()],
                revision: 9,
            }),
            json!({
                "vocabulary_changed": true,
                "query_impact": tags_impact_json,
                "asset_id": 7,
                "changed": true,
                "tags": ["cat"],
                "revision": 9
            })
        );
        assert_eq!(
            to_json(BulkTagMergeSummary {
                vocabulary_changed: false,
                query_impact: TagQueryImpact::None,
                processed_assets: 2,
                updated_assets: 1,
                processed_asset_ids: vec![7, 8],
                updated_asset_ids: vec![7],
                results: vec![AssetTagResult {
                    asset_id: 7,
                    changed: true,
                    tags: vec!["cat".into()],
                }],
                revision: 9,
            }),
            json!({
                "vocabulary_changed": false,
                "query_impact": { "type": "none" },
                "processed_assets": 2,
                "updated_assets": 1,
                "processed_asset_ids": [7, 8],
                "updated_asset_ids": [7],
                "results": [{ "asset_id": 7, "changed": true, "tags": ["cat"] }],
                "revision": 9
            })
        );
        assert_eq!(
            to_json(SelectionMutationResult::Ready {
                processed_assets: 2,
                updated_assets: 2,
                revision: 9,
                query_impact: TagQueryImpact::None,
                vocabulary_changed: false,
                is_favorite: Some(true),
                media_group_key: None,
            }),
            json!({
                "status": "ready",
                "processed_assets": 2,
                "updated_assets": 2,
                "revision": 9,
                "query_impact": { "type": "none" },
                "vocabulary_changed": false,
                "is_favorite": true,
                "media_group_key": null
            })
        );
        assert_eq!(
            to_json(SelectionMutationResult::Stale),
            json!({ "status": "stale" })
        );
        assert_eq!(
            to_json(BulkFavoriteSummary {
                processed_asset_ids: vec![7],
                is_favorite: true,
                revision: 9,
            }),
            json!({ "processed_asset_ids": [7], "is_favorite": true, "revision": 9 })
        );
        assert_eq!(
            to_json(BulkMediaGroupSummary {
                processed_asset_ids: vec![7],
                processed_assets: 1,
                updated_assets: 1,
                media_group_key: Some("g".into()),
            }),
            json!({
                "processed_asset_ids": [7],
                "processed_assets": 1,
                "updated_assets": 1,
                "media_group_key": "g"
            })
        );
    }

    #[test]
    fn delete_and_duplicate_results() {
        let statuses = [
            (DeleteSourceStatus::Deleted, "deleted"),
            (DeleteSourceStatus::Missing, "missing"),
            (DeleteSourceStatus::CleanupPending, "cleanup_pending"),
        ];
        for (status, name) in statuses {
            assert_eq!(to_json(status), json!(name));
        }
        assert_eq!(
            to_json(DeleteAssetSummary {
                removed_assets: 1,
                removed_thumbnails: 1,
                source_status: DeleteSourceStatus::Deleted,
                revision: 9,
                recovery_path: None,
            }),
            json!({
                "removed_assets": 1,
                "removed_thumbnails": 1,
                "source_status": "deleted",
                "revision": 9,
                "recovery_path": null
            })
        );
        assert_eq!(
            to_json(DuplicateScanSummary {
                groups: vec![DuplicateGroup {
                    file_name: "a.jpg".into(),
                    assets: vec![DuplicateAsset {
                        id: 7,
                        path: "/media/a.jpg".into(),
                        record_version: 2,
                        size_bytes: 1234,
                        thumb_version: "0123456789abcdef".into(),
                    }],
                }],
                duplicate_groups: 1,
                duplicate_assets: 2,
                revision: 9,
            }),
            json!({
                "groups": [{
                    "file_name": "a.jpg",
                    "assets": [{
                        "id": 7,
                        "path": "/media/a.jpg",
                        "record_version": 2,
                        "size_bytes": 1234,
                        "thumb_version": "0123456789abcdef"
                    }]
                }],
                "duplicate_groups": 1,
                "duplicate_assets": 2,
                "revision": 9
            })
        );
        for (status, name) in [
            (RenameAssetStatus::Renamed, "renamed"),
            (RenameAssetStatus::CleanupPending, "cleanup_pending"),
        ] {
            assert_eq!(to_json(status), json!(name));
        }
        assert_eq!(
            to_json(RenameAssetSummary {
                asset_id: 7,
                old_path: "/media/a.jpg".into(),
                new_path: "/media/b.jpg".into(),
                removed_thumbnails: 0,
                revision: 9,
                status: RenameAssetStatus::Renamed,
                recovery_path: None,
            }),
            json!({
                "asset_id": 7,
                "old_path": "/media/a.jpg",
                "new_path": "/media/b.jpg",
                "removed_thumbnails": 0,
                "revision": 9,
                "status": "renamed",
                "recovery_path": null
            })
        );
        for (status, name) in [
            (DuplicateResolutionBatchStatus::Committed, "committed"),
            (DuplicateResolutionBatchStatus::RolledBack, "rolled_back"),
            (
                DuplicateResolutionBatchStatus::RecoveryRequired,
                "recovery_required",
            ),
        ] {
            assert_eq!(to_json(status), json!(name));
        }
        for (status, name) in [
            (DuplicateResolutionItemStatus::Renamed, "renamed"),
            (DuplicateResolutionItemStatus::Deleted, "deleted"),
            (
                DuplicateResolutionItemStatus::SourceMissing,
                "source_missing",
            ),
            (DuplicateResolutionItemStatus::RolledBack, "rolled_back"),
            (
                DuplicateResolutionItemStatus::RollbackFailed,
                "rollback_failed",
            ),
            (
                DuplicateResolutionItemStatus::CleanupPending,
                "cleanup_pending",
            ),
        ] {
            assert_eq!(to_json(status), json!(name));
        }
        assert_eq!(
            to_json(DuplicateResolutionBatchSummary {
                status: DuplicateResolutionBatchStatus::Committed,
                revision: 9,
                results: vec![DuplicateResolutionItemResult {
                    asset_id: 7,
                    status: DuplicateResolutionItemStatus::Renamed,
                    old_path: "/media/a.jpg".into(),
                    new_path: Some("/media/b.jpg".into()),
                    recovery_path: None,
                }],
                removed_thumbnails: 1,
            }),
            json!({
                "status": "committed",
                "revision": 9,
                "results": [{
                    "asset_id": 7,
                    "status": "renamed",
                    "old_path": "/media/a.jpg",
                    "new_path": "/media/b.jpg",
                    "recovery_path": null
                }],
                "removed_thumbnails": 1
            })
        );
    }

    #[test]
    fn scan_thumbnail_and_library_results() {
        assert_eq!(to_json(ScanCompletion::Complete), json!("complete"));
        assert_eq!(
            to_json(ScanSummary {
                completion: ScanCompletion::Partial,
                indexed: 3,
                removed: 1,
                failed: 2,
            }),
            json!({ "completion": "partial", "indexed": 3, "removed": 1, "failed": 2 })
        );
        assert_eq!(
            to_json(RemoveRootSummary {
                removed_assets: 3,
                removed_thumbnails: 2,
            }),
            json!({ "removed_assets": 3, "removed_thumbnails": 2 })
        );
        assert_eq!(
            to_json(ThumbnailRenderSummary {
                stale: 1,
                generated: 2,
                failed: 3,
                skipped_failed: 4,
                processed: 5,
                total: 6,
                cancelled: true,
            }),
            json!({
                "stale": 1,
                "generated": 2,
                "failed": 3,
                "skipped_failed": 4,
                "processed": 5,
                "total": 6,
                "cancelled": true
            })
        );
        assert_eq!(
            to_json(VideoToolStatus {
                ffmpeg_available: true,
                ffprobe_available: false,
            }),
            json!({ "ffmpeg_available": true, "ffprobe_available": false })
        );
        assert_eq!(
            to_json(ClearLibrarySummary {
                removed_assets: 3,
                removed_roots: 1,
                removed_thumbnails: 2,
            }),
            json!({ "removed_assets": 3, "removed_roots": 1, "removed_thumbnails": 2 })
        );
        // Payload of the `process-progress` event.
        assert_eq!(
            to_json(ScanProgress {
                phase: "scan".into(),
                processed: 1,
                total: 2,
                message: "m".into(),
            }),
            json!({ "phase": "scan", "processed": 1, "total": 2, "message": "m" })
        );
    }

    #[test]
    fn import_export_results() {
        assert_eq!(to_json(CsvExportSummary { rows: 2 }), json!({ "rows": 2 }));
        assert_eq!(
            to_json(CsvImportSummary {
                rows_read: 4,
                rows_applied: 3,
                assets_matched: 2,
                assets_updated: 1,
            }),
            json!({
                "rows_read": 4,
                "rows_applied": 3,
                "assets_matched": 2,
                "assets_updated": 1
            })
        );
        assert_eq!(
            to_json(DbBundleExportSummary {
                copied_files: 2,
                copied_thumbnails: 1,
            }),
            json!({ "copied_files": 2, "copied_thumbnails": 1 })
        );
        assert_eq!(
            to_json(DbBundleImportSummary {
                restored_files: 2,
                restored_thumbnails: 1,
            }),
            json!({ "restored_files": 2, "restored_thumbnails": 1 })
        );
        assert_eq!(
            to_json(DbBundleInspection {
                format_version: Some(2),
                source_platform: Some("windows".into()),
                source_thumbs_dir: None,
                roots: vec!["C:\\media".into()],
                requires_mapping: true,
            }),
            json!({
                "format_version": 2,
                "source_platform": "windows",
                "source_thumbs_dir": null,
                "roots": ["C:\\media"],
                "requires_mapping": true
            })
        );
        assert_eq!(
            to_json(DbRootMapping {
                source_root: "C:\\media".into(),
                target_root: "/media".into(),
            }),
            json!({ "sourceRoot": "C:\\media", "targetRoot": "/media" })
        );
    }

    /// Messages of the `open_video` channel.
    #[test]
    fn video_channel_events() {
        let event = |payload| {
            to_json(VideoEvent {
                session_id: 4,
                payload,
            })
        };
        assert_eq!(
            event(VideoEventPayload::Escape),
            json!({ "session_id": 4, "type": "escape" })
        );
        assert_eq!(
            event(VideoEventPayload::PointerActivity),
            json!({ "session_id": 4, "type": "pointerActivity" })
        );
        assert_eq!(
            event(VideoEventPayload::Loading),
            json!({ "session_id": 4, "type": "loading" })
        );
        assert_eq!(
            event(VideoEventPayload::Metadata {
                duration: 1.5,
                width: 640,
                height: 480,
            }),
            json!({
                "session_id": 4,
                "type": "metadata",
                "duration": 1.5,
                "width": 640,
                "height": 480
            })
        );
        assert_eq!(
            event(VideoEventPayload::Snapshot {
                state: PlaybackSnapshot {
                    session_id: 4,
                    duration: 10.0,
                    current_time: 2.5,
                    paused: false,
                    seeking: false,
                    buffering: true,
                    volume: 0.5,
                    muted: false,
                    rate: 1.0,
                    fullscreen: false,
                },
            }),
            json!({
                "session_id": 4,
                "type": "snapshot",
                "state": {
                    "session_id": 4,
                    "duration": 10.0,
                    "current_time": 2.5,
                    "paused": false,
                    "seeking": false,
                    "buffering": true,
                    "volume": 0.5,
                    "muted": false,
                    "rate": 1.0,
                    "fullscreen": false
                }
            })
        );
        assert_eq!(
            event(VideoEventPayload::Fullscreen { fullscreen: true }),
            json!({ "session_id": 4, "type": "fullscreen", "fullscreen": true })
        );
        assert_eq!(
            event(VideoEventPayload::Error {
                message: "m".into(),
            }),
            json!({ "session_id": 4, "type": "error", "message": "m" })
        );
        assert_eq!(
            event(VideoEventPayload::ControlError {
                message: "m".into(),
            }),
            json!({ "session_id": 4, "type": "controlError", "message": "m" })
        );
    }

    #[test]
    fn query_and_selection_inputs() {
        match from_json(json!({ "type": "hasNoTags", "tagCount": 2 })) {
            AssetMetaFilterInput::HasNoTags { tag_count } => assert_eq!(tag_count, 2),
            other => panic!("unexpected {other:?}"),
        }
        match from_json(json!({ "type": "groupName", "groupName": "g" })) {
            AssetMetaFilterInput::GroupName { group_name } => assert_eq!(group_name, "g"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            serde_json::from_value::<AssetMetaFilterInput>(json!({ "type": "other" })).is_err()
        );

        let spec: SelectionSpec = from_json(json!({
            "sessionId": 3,
            "ranges": [[0, 9]],
            "excluded": [4],
            "included": [12]
        }));
        assert_eq!(
            (spec.session_id, spec.ranges, spec.excluded, spec.included),
            (3, vec![[0, 9]], vec![4], vec![12])
        );
        let minimal: SelectionSpec = from_json(json!({ "sessionId": 3, "ranges": [] }));
        assert!(minimal.excluded.is_empty() && minimal.included.is_empty());

        match from_json(json!({ "type": "mergeTags", "tags": ["cat"] })) {
            SelectionMutation::MergeTags { tags } => assert_eq!(tags, ["cat"]),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            from_json(json!({ "type": "toggleFavorite" })),
            SelectionMutation::ToggleFavorite
        ));
        match from_json(json!({ "type": "setMediaGroup", "mediaGroupKey": "g" })) {
            SelectionMutation::SetMediaGroup { media_group_key } => {
                assert_eq!(media_group_key.as_deref(), Some("g"))
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(serde_json::from_value::<SelectionMutation>(json!({ "type": "other" })).is_err());

        let update: BulkMediaGroupUpdateInput =
            from_json(json!({ "assetId": 7, "mediaGroupOrder": 2.0 }));
        assert_eq!((update.asset_id, update.media_group_order), (7, Some(2.0)));
    }

    #[test]
    fn duplicate_and_restore_inputs() {
        let batch: DuplicateResolutionBatchInput = from_json(json!({
            "scanRevision": 9,
            "changes": [
                {
                    "type": "rename",
                    "assetId": 7,
                    "expectedPath": "/media/a.jpg",
                    "expectedRecordVersion": 2,
                    "newFileName": "b.jpg"
                },
                {
                    "type": "delete",
                    "assetId": 8,
                    "expectedPath": "/media/c.jpg",
                    "expectedRecordVersion": 3
                }
            ]
        }));
        assert_eq!(batch.scan_revision, 9);
        match &batch.changes[..] {
            [DuplicateResolutionChangeInput::Rename {
                asset_id: 7,
                expected_path: rename_path,
                expected_record_version: 2,
                new_file_name,
            }, DuplicateResolutionChangeInput::Delete {
                asset_id: 8,
                expected_path: delete_path,
                expected_record_version: 3,
            }] => {
                assert_eq!(rename_path, "/media/a.jpg");
                assert_eq!(new_file_name, "b.jpg");
                assert_eq!(delete_path, "/media/c.jpg");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(
            serde_json::from_value::<DuplicateResolutionChangeInput>(json!({
                "type": "move",
                "assetId": 7,
                "expectedPath": "/media/a.jpg",
                "expectedRecordVersion": 2
            }))
            .is_err()
        );

        let mapping: DbRootMapping =
            from_json(json!({ "sourceRoot": "C:\\media", "targetRoot": "/media" }));
        assert_eq!(
            (mapping.source_root.as_str(), mapping.target_root.as_str()),
            ("C:\\media", "/media")
        );
    }

    #[test]
    fn video_inputs() {
        let bounds: VideoBounds =
            from_json(json!({ "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0 }));
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (1.0, 2.0, 3.0, 4.0)
        );

        let labels: VideoControlLabels = from_json(json!({
            "play": "a",
            "pause": "b",
            "mute": "c",
            "unmute": "d",
            "volume": "e",
            "seek": "f",
            "playbackRate": "g",
            "fullscreen": "h",
            "exitFullscreen": "i"
        }));
        assert_eq!(
            [
                labels.play,
                labels.pause,
                labels.mute,
                labels.unmute,
                labels.volume,
                labels.seek,
                labels.playback_rate,
                labels.fullscreen,
                labels.exit_fullscreen,
            ],
            ["a", "b", "c", "d", "e", "f", "g", "h", "i"]
        );

        let control = |value: Value| from_json::<VideoControl>(value);
        assert!(matches!(
            control(json!({ "type": "play" })),
            VideoControl::Play
        ));
        assert!(matches!(
            control(json!({ "type": "pause" })),
            VideoControl::Pause
        ));
        assert!(matches!(
            control(json!({ "type": "togglePause" })),
            VideoControl::TogglePause
        ));
        assert!(matches!(
            control(json!({ "type": "seek", "time": 1.5 })),
            VideoControl::Seek { time } if time == 1.5
        ));
        assert!(matches!(
            control(json!({ "type": "seekRelative", "seconds": -5.0 })),
            VideoControl::SeekRelative { seconds } if seconds == -5.0
        ));
        assert!(matches!(
            control(json!({ "type": "setVolume", "volume": 0.5 })),
            VideoControl::SetVolume { volume } if volume == 0.5
        ));
        assert!(matches!(
            control(json!({ "type": "setMuted", "muted": true })),
            VideoControl::SetMuted { muted: true }
        ));
        assert!(matches!(
            control(json!({ "type": "setRate", "rate": 2.0 })),
            VideoControl::SetRate { rate } if rate == 2.0
        ));
        assert!(matches!(
            control(json!({ "type": "selectAudioTrack", "trackId": "1" })),
            VideoControl::SelectAudioTrack { track_id } if track_id == "1"
        ));
        assert!(matches!(
            control(json!({ "type": "selectSubtitleTrack", "trackId": "2" })),
            VideoControl::SelectSubtitleTrack { track_id } if track_id == "2"
        ));
        assert!(matches!(
            control(json!({ "type": "setFullscreen", "fullscreen": true })),
            VideoControl::SetFullscreen { fullscreen: true }
        ));
        assert!(matches!(
            control(json!({ "type": "toggleFullscreen" })),
            VideoControl::ToggleFullscreen
        ));
        assert!(serde_json::from_value::<VideoControl>(json!({ "type": "stop" })).is_err());
    }
}
