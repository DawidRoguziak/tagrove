//! The client crate stays free of any GUI runtime in its whole dependency closure, not
//! only in its own manifest (acceptance criterion of epic E00: `cargo tree -p
//! media_tagger_client -e normal` names no gpui, wgpu, blade, tauri, gtk, webkit or
//! libmpv). The closure is read from `src-gpui/Cargo.lock`, which also lists dev,
//! build and every-platform dependencies, so it is a superset of that tree.

use std::collections::{BTreeSet, HashMap};

const LOCK: &str = include_str!("../../../Cargo.lock");

/// Prefixes of the GUI stacks of both adapters.
const GUI_PREFIXES: &[&str] = &[
    "gpui", "wgpu", "blade", "tauri", "gtk", "gdk", "glib", "webkit", "libmpv", "wry",
];

#[derive(Debug)]
struct Package {
    name: String,
    version: String,
    /// `name` or `name version` as the lock file writes them.
    dependencies: Vec<String>,
}

fn quoted(line: &str) -> Option<&str> {
    let start = line.find('"')? + 1;
    let end = start + line[start..].find('"')?;
    Some(&line[start..end])
}

fn packages(lock: &str) -> Vec<Package> {
    lock.split("[[package]]")
        .skip(1)
        .map(|block| {
            let mut package = Package {
                name: String::new(),
                version: String::new(),
                dependencies: Vec::new(),
            };
            let mut in_dependencies = false;
            for line in block.lines().map(str::trim) {
                if in_dependencies {
                    if line.starts_with(']') {
                        in_dependencies = false;
                    } else if let Some(dependency) = quoted(line) {
                        package.dependencies.push(dependency.to_owned());
                    }
                } else if let Some(rest) = line.strip_prefix("name = ") {
                    package.name = quoted(rest).unwrap_or_default().to_owned();
                } else if let Some(rest) = line.strip_prefix("version = ") {
                    package.version = quoted(rest).unwrap_or_default().to_owned();
                } else if line.starts_with("dependencies = [") {
                    in_dependencies = !line.ends_with(']');
                }
            }
            package
        })
        .collect()
}

/// Names of every package reachable from `root` in the lock file.
fn closure(lock: &str, root: &str) -> BTreeSet<String> {
    let packages = packages(lock);
    let mut by_name: HashMap<&str, Vec<&Package>> = HashMap::new();
    for package in &packages {
        by_name.entry(&package.name).or_default().push(package);
    }
    let mut seen = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut stack: Vec<&Package> = by_name.get(root).cloned().unwrap_or_default();
    while let Some(package) = stack.pop() {
        if !seen.insert((package.name.as_str(), package.version.as_str())) {
            continue;
        }
        names.insert(package.name.clone());
        for dependency in &package.dependencies {
            let mut parts = dependency.split(' ');
            let name = parts.next().unwrap_or_default();
            let version = parts.next();
            let candidates = by_name.get(name).map(Vec::as_slice).unwrap_or_default();
            assert!(
                !candidates.is_empty(),
                "{dependency} is not in the lock file"
            );
            stack.extend(
                candidates
                    .iter()
                    .filter(|candidate| version.is_none_or(|v| candidate.version == v)),
            );
        }
    }
    names
}

fn gui_packages(names: &BTreeSet<String>) -> Vec<&String> {
    names
        .iter()
        .filter(|name| GUI_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) || *name == "tao")
        .collect()
}

#[test]
fn the_client_closure_contains_no_gui_runtime() {
    let names = closure(LOCK, "media_tagger_client");

    for expected in ["media_tagger_core", "rusqlite", "serde_json"] {
        assert!(
            names.contains(expected),
            "the closure misses {expected}: {names:?}"
        );
    }
    assert_eq!(
        gui_packages(&names),
        Vec::<&String>::new(),
        "media_tagger_client reaches a GUI runtime through its dependencies"
    );
}

#[test]
fn the_check_sees_the_gui_stack_of_the_adapter() {
    let names = closure(LOCK, "media_tagger_gpui");

    let gui = gui_packages(&names);
    assert!(gui.iter().any(|name| *name == "gpui"), "{gui:?}");
    assert!(gui.iter().any(|name| name.starts_with("wgpu")), "{gui:?}");
}
