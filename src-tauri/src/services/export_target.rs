//! Validation of the user-chosen target shared by CSV export and bundle export.
//!
//! The target is resolved and checked before anything is created on disk, so a
//! rejected export never leaves directories behind (ADR 0017).

use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
};

use rusqlite::Connection;

use crate::{db, error::AppResult};

/// Resolves `raw_path`, rejects a target inside `profile_dir`, inside an indexed
/// scan root, or equal to the real path of an indexed asset, and only then
/// creates the missing parent directories. `label` prefixes every error, for
/// example "CSV export" or "DB export archive".
pub(crate) fn prepare_export_target(
    raw_path: &str,
    profile_dir: &Path,
    conn: &Connection,
    label: &str,
) -> AppResult<PathBuf> {
    let target = resolve_export_target(raw_path, profile_dir, conn, label)?;
    let parent = target
        .parent()
        .ok_or_else(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent)?;
    // The checks ran on a lexically resolved path. If a folder appeared as a
    // symbolic link in the meantime, the real target is not the checked one.
    if parent.canonicalize()? != parent {
        return Err(format!("{label} target folder changed during validation").into());
    }
    Ok(target)
}

fn resolve_export_target(
    raw_path: &str,
    profile_dir: &Path,
    conn: &Connection,
    label: &str,
) -> AppResult<PathBuf> {
    let requested = Path::new(raw_path);
    if requested.as_os_str().is_empty() {
        return Err(format!("{label} path is empty").into());
    }
    let file_name = requested
        .file_name()
        .ok_or_else(|| format!("{label} path has no file name"))?;
    let parent = requested
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let target = canonicalize_allowing_missing(parent)?.join(file_name);

    if target.starts_with(profile_dir.canonicalize()?) {
        return Err(format!("{label} cannot be stored inside the active profile").into());
    }
    // A missing root (an unmounted drive, a deleted folder) resolves the same way
    // as the target, so a target below it is still rejected before anything
    // could be created there.
    for root in db::list_scan_roots(conn)? {
        if let Ok(root) = resolve_root(Path::new(&root)) {
            if target.starts_with(root) {
                return Err(
                    format!("{label} cannot be stored inside an indexed source root").into(),
                );
            }
        }
    }
    if is_real_path_of_indexed_asset(conn, &target)? {
        return Err(format!("{label} cannot overwrite an indexed source file").into());
    }
    Ok(target)
}

/// How [`resolve_allowing_missing`] treats a symbolic link whose destination
/// does not exist.
#[derive(Clone, Copy)]
enum DanglingLinks {
    /// An export target: folders would be created through the link, so refuse.
    Reject,
    /// A registered root: follow the link to where the root would be, so a
    /// root reached through a link whose destination is gone still protects it.
    Follow,
}

/// Upper bound on followed dangling links, as the kernel's `ELOOP` limit.
const MAX_DANGLING_LINK_HOPS: usize = 40;

/// Resolves an export target's parent folder; see [`resolve_allowing_missing`].
fn canonicalize_allowing_missing(path: &Path) -> io::Result<PathBuf> {
    resolve_allowing_missing(path, DanglingLinks::Reject, 0)
}

/// Resolves a registered root; see [`resolve_allowing_missing`].
fn resolve_root(path: &Path) -> io::Result<PathBuf> {
    resolve_allowing_missing(path, DanglingLinks::Follow, 0)
}

/// Resolves `path` one component at a time, the way the kernel will once the
/// missing folders exist. `resolved` never contains a symbolic link: a
/// component that exists is canonicalized (following any link), a missing one
/// is appended as a plain name, and `..` drops the last resolved component.
/// Because an existing component is canonicalized even after a missing folder
/// and `..`, a symbolic link reached that way is still followed. A dangling
/// symbolic link is rejected or followed according to `dangling`.
fn resolve_allowing_missing(
    path: &Path,
    dangling: DanglingLinks,
    hops: usize,
) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => resolved.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                let candidate = resolved.join(name);
                resolved = match candidate.canonicalize() {
                    // Every component of a folder path is a folder; like the
                    // kernel, refuse `file/..` instead of resolving it lexically.
                    Ok(canonical) if !canonical.is_dir() => {
                        return Err(io::Error::new(
                            io::ErrorKind::NotADirectory,
                            "export path uses a file as a folder",
                        ));
                    }
                    Ok(canonical) => canonical,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        match (fs::read_link(&candidate), dangling) {
                            (Err(_), _) => candidate,
                            (Ok(_), DanglingLinks::Reject) => {
                                return Err(io::Error::new(
                                    io::ErrorKind::NotFound,
                                    "export path contains a dangling symbolic link",
                                ));
                            }
                            (Ok(_), DanglingLinks::Follow) if hops >= MAX_DANGLING_LINK_HOPS => {
                                return Err(io::Error::other("too many levels of symbolic links"));
                            }
                            // A relative destination is relative to the link's
                            // folder, which `resolved` still names.
                            (Ok(destination), DanglingLinks::Follow) => resolve_allowing_missing(
                                &resolved.join(destination),
                                dangling,
                                hops + 1,
                            )?,
                        }
                    }
                    Err(error) => return Err(error),
                };
            }
        }
    }
    Ok(resolved)
}

/// Whether some indexed asset path canonicalizes to `target`, whose parent is
/// already canonical.
///
/// A canonical path names an existing entry that is not a symbolic link, so a
/// missing target or a symbolic-link target can never match and costs no asset
/// lookup. Otherwise an exact `assets.path` match is found through its unique
/// index; an alias (an asset reached through a symbolic link) is found by
/// comparing device and inode with one `stat` per asset, and a candidate is
/// confirmed with `canonicalize` so a hard link is not mistaken for the asset.
fn is_real_path_of_indexed_asset(conn: &Connection, target: &Path) -> AppResult<bool> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Ok(false);
    }
    if let Some(path) = target.to_str() {
        if db::asset_path_exists(conn, path)? {
            return Ok(true);
        }
    }
    let (device, inode) = (metadata.dev(), metadata.ino());
    Ok(db::any_asset_path(conn, |path| {
        fs::metadata(path).is_ok_and(|source| source.dev() == device && source.ino() == inode)
            && Path::new(path)
                .canonicalize()
                .is_ok_and(|source| source == target)
    })?)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
    };

    use tempfile::{tempdir, TempDir};

    use super::{canonicalize_allowing_missing, prepare_export_target};
    use crate::{db, models::NewAsset};

    struct Fixture {
        tmp: TempDir,
        conn: rusqlite::Connection,
    }

    impl Fixture {
        fn new() -> Self {
            let tmp = tempdir().unwrap();
            fs::create_dir_all(tmp.path().join("profile")).unwrap();
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            db::init_schema(&conn).unwrap();
            Self { tmp, conn }
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.tmp.path().join(relative)
        }

        fn index_asset(&self, path: &Path) {
            db::upsert_asset(
                &self.conn,
                &NewAsset {
                    path: path.to_string_lossy().into_owned(),
                    kind: "image".to_string(),
                    size_bytes: 1,
                    modified_at: 1,
                    width: None,
                    height: None,
                    duration_ms: None,
                    thumb_path: None,
                },
            )
            .unwrap();
        }

        fn prepare(&self, target: &Path) -> Result<PathBuf, String> {
            prepare_export_target(
                target.to_str().unwrap(),
                &self.path("profile"),
                &self.conn,
                "Test export",
            )
            .map_err(|error| error.to_string())
        }

        /// Every directory and file below the fixture root, as relative paths.
        fn tree(&self) -> Vec<String> {
            let mut entries: Vec<String> = walkdir::WalkDir::new(self.tmp.path())
                .min_depth(1)
                .into_iter()
                .map(|entry| {
                    let entry = entry.unwrap();
                    entry
                        .path()
                        .strip_prefix(self.tmp.path())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            entries.sort();
            entries
        }
    }

    /// The per-asset check this module replaced: reject when any stored asset
    /// path canonicalizes to the canonical target.
    fn previous_asset_check_rejects(conn: &rusqlite::Connection, target: &Path) -> bool {
        db::list_asset_paths(conn)
            .unwrap()
            .into_iter()
            .any(|asset| Path::new(&asset.path).canonicalize().ok().as_deref() == Some(target))
    }

    #[test]
    fn rejected_targets_with_missing_parents_create_no_directory() {
        let fixture = Fixture::new();
        let root = fixture.path("media");
        fs::create_dir_all(&root).unwrap();
        db::add_scan_root(&fixture.conn, root.to_str().unwrap()).unwrap();
        let before = fixture.tree();

        let in_profile = fixture.prepare(&fixture.path("profile/new/deeper/out.csv"));
        assert!(in_profile
            .unwrap_err()
            .contains("inside the active profile"));
        let in_root = fixture.prepare(&root.join("new/deeper/out.csv"));
        assert!(in_root
            .unwrap_err()
            .contains("inside an indexed source root"));
        // A missing folder followed by `..` must not leave the missing folder behind.
        let via_parent = fixture.prepare(&fixture.path("stray/../media/out.csv"));
        assert!(via_parent
            .unwrap_err()
            .contains("inside an indexed source root"));

        assert_eq!(fixture.tree(), before);
    }

    #[test]
    fn a_registered_but_missing_root_still_rejects_targets_below_it() {
        let fixture = Fixture::new();
        let unmounted = fixture.path("unmounted/media");
        db::add_scan_root(&fixture.conn, unmounted.to_str().unwrap()).unwrap();
        let before = fixture.tree();

        let below_root = fixture.prepare(&unmounted.join("sub/out.csv"));
        assert!(below_root
            .unwrap_err()
            .contains("inside an indexed source root"));
        let via_parent = fixture.prepare(&fixture.path("stray/../unmounted/media/out.csv"));
        assert!(via_parent
            .unwrap_err()
            .contains("inside an indexed source root"));

        assert_eq!(fixture.tree(), before);
        assert!(fixture
            .prepare(&fixture.path("unmounted/elsewhere/out.csv"))
            .is_ok());
    }

    #[test]
    fn a_root_registered_through_a_dangling_symlink_protects_its_destination() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.path("data")).unwrap();
        // Relative and absolute link destinations, chained, all currently missing.
        symlink("data/Media", fixture.path("root-link")).unwrap();
        symlink(fixture.path("chain-hop"), fixture.path("chain-link")).unwrap();
        symlink("data/Other", fixture.path("chain-hop")).unwrap();
        symlink("loop-b", fixture.path("loop-a")).unwrap();
        symlink("loop-a", fixture.path("loop-b")).unwrap();
        for root in ["root-link", "chain-link", "loop-a"] {
            db::add_scan_root(&fixture.conn, fixture.path(root).to_str().unwrap()).unwrap();
        }
        let before = fixture.tree();

        for target in ["data/Media/new/out.csv", "data/Other/out.csv"] {
            let result = fixture.prepare(&fixture.path(target));
            assert!(
                result
                    .unwrap_err()
                    .contains("inside an indexed source root"),
                "{target}"
            );
        }
        // The link path itself is a dangling link in the target, refused as before.
        assert!(fixture.prepare(&fixture.path("root-link/out.csv")).is_err());

        assert_eq!(fixture.tree(), before);
        // A link loop only drops that root; other targets stay usable.
        assert!(fixture
            .prepare(&fixture.path("data/elsewhere/out.csv"))
            .is_ok());
    }

    #[test]
    fn accepted_targets_create_only_the_resolved_parent() {
        let fixture = Fixture::new();
        let target = fixture.prepare(&fixture.path("exports/stray/../a/b/out.csv"));

        let expected = fixture
            .tmp
            .path()
            .canonicalize()
            .unwrap()
            .join("exports/a/b/out.csv");
        assert_eq!(target.unwrap(), expected);
        assert_eq!(
            fixture.tree(),
            vec!["exports", "exports/a", "exports/a/b", "profile"]
        );
    }

    #[test]
    fn a_dangling_symlink_in_a_missing_parent_never_creates_inside_a_root() {
        let fixture = Fixture::new();
        let root = fixture.path("media");
        fs::create_dir_all(&root).unwrap();
        db::add_scan_root(&fixture.conn, root.to_str().unwrap()).unwrap();
        symlink(root.join("missing"), fixture.path("link")).unwrap();
        let before = fixture.tree();

        assert!(fixture.prepare(&fixture.path("link/out.csv")).is_err());

        assert_eq!(fixture.tree(), before);
        assert!(!root.join("missing").exists());
    }

    #[test]
    fn a_symlink_reached_after_a_missing_folder_and_parent_is_followed() {
        let fixture = Fixture::new();
        let root = fixture.path("media");
        fs::create_dir_all(&root).unwrap();
        db::add_scan_root(&fixture.conn, root.to_str().unwrap()).unwrap();
        symlink(&root, fixture.path("link")).unwrap();
        symlink(fixture.path("profile"), fixture.path("profile-link")).unwrap();
        let before = fixture.tree();

        let into_root = fixture.prepare(&fixture.path("missing/../link/new/out.csv"));
        assert!(into_root
            .unwrap_err()
            .contains("inside an indexed source root"));
        let into_profile = fixture.prepare(&fixture.path("missing/../profile-link/new/out.csv"));
        assert!(into_profile
            .unwrap_err()
            .contains("inside the active profile"));

        assert_eq!(fixture.tree(), before);
    }

    #[test]
    fn indexed_path_check_rejects_exactly_what_the_per_asset_loop_rejected() {
        let fixture = Fixture::new();
        let outside = fixture.path("outside");
        let aliases = fixture.path("aliases");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&aliases).unwrap();
        let direct = outside.join("direct.jpg");
        let file_alias_target = outside.join("behind-file-link.jpg");
        let dir_alias_target = outside.join("behind-dir-link.jpg");
        let hard_linked = outside.join("hard-linked.jpg");
        let hard_link_source = outside.join("hard-link-source.jpg");
        for file in [
            &direct,
            &file_alias_target,
            &dir_alias_target,
            &hard_link_source,
        ] {
            fs::write(file, b"media").unwrap();
        }
        fs::hard_link(&hard_link_source, &hard_linked).unwrap();
        symlink(&file_alias_target, aliases.join("file-link.jpg")).unwrap();
        symlink(&outside, aliases.join("dir-link")).unwrap();
        symlink(&direct, outside.join("link-to-direct.jpg")).unwrap();
        // No scan root covers these rows, so only the indexed-path check can reject.
        fixture.index_asset(&direct);
        fixture.index_asset(&aliases.join("file-link.jpg"));
        fixture.index_asset(&aliases.join("dir-link/behind-dir-link.jpg"));
        fixture.index_asset(&hard_link_source);
        fixture.index_asset(&fixture.path("gone/missing.jpg"));

        let canonical_outside = outside.canonicalize().unwrap();
        let cases = [
            ("direct.jpg", true),
            ("behind-file-link.jpg", true),
            ("behind-dir-link.jpg", true),
            ("hard-link-source.jpg", true),
            ("hard-linked.jpg", false),
            ("link-to-direct.jpg", false),
            ("new.csv", false),
            ("missing.jpg", false),
        ];
        for (name, rejected) in cases {
            let target = canonical_outside.join(name);
            assert_eq!(
                previous_asset_check_rejects(&fixture.conn, &target),
                rejected,
                "reference {name}"
            );
            let result = fixture.prepare(&outside.join(name));
            assert_eq!(result.is_err(), rejected, "{name}: {result:?}");
            if rejected {
                assert!(result
                    .unwrap_err()
                    .contains("cannot overwrite an indexed source file"));
            }
        }
    }

    #[test]
    fn relative_and_missing_parents_resolve_component_by_component() {
        let fixture = Fixture::new();
        let base = fixture.tmp.path().canonicalize().unwrap();
        assert_eq!(
            canonicalize_allowing_missing(&fixture.path("a/./b/../c")).unwrap(),
            base.join("a/c")
        );
        let file = fixture.path("file");
        fs::write(&file, b"x").unwrap();
        assert!(canonicalize_allowing_missing(&file.join("below")).is_err());
        assert!(canonicalize_allowing_missing(&file.join("..").join("x")).is_err());
        assert_eq!(
            canonicalize_allowing_missing(Path::new(".")).unwrap(),
            std::env::current_dir().unwrap().canonicalize().unwrap()
        );
    }
}
