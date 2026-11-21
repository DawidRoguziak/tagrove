//! Comparison scenarios: named, deterministic start states of the application, so that
//! the Tauri and the GPUI interface can be captured in the same state on the same
//! synthetic data (`bun run compare:ui`, plan decision D10).
//!
//! The catalogue of all scenarios is `scenarios` in `docs/plans/gpui-port/epics.json`.
//! Each scenario this crate knows has a counterpart `e2e/compare/scenarios/<id>.js`,
//! which holds the steps that bring the Tauri application into the same state; the
//! tests below keep the three in step. A UI epic adds its scenarios here together with
//! the state they need.
//!
//! This module only describes the states. Whether a process may start in one is decided
//! by the adapter: only the isolated E2E profile accepts a scenario.

use crate::{
    runtime::{DEFAULT_TILE_SIZE, TILE_SIZE_MAX, TILE_SIZE_MIN},
    theme::ThemeName,
};

/// The synthetic media a scenario starts with (`seed` of its file in
/// `e2e/compare/scenarios`, `e2e/compare/seed.js`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seed {
    /// No scan root and no asset.
    Empty,
    /// The media `app:control` indexes for the Tauri application: four PNGs, one GIF
    /// and two MP4s in one scan root. The harness writes them into a fresh directory
    /// and names it in [`SCENARIO_MEDIA_VARIABLE`].
    Control,
}

impl Seed {
    /// The name in the scenario files.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Control => "control",
        }
    }
}

/// The environment variable that names the seeded media directory of the running
/// scenario (`SCENARIO_MEDIA_VARIABLE` of `e2e/compare/gpui-control-support.js`).
pub const SCENARIO_MEDIA_VARIABLE: &str = "MEDIATAGGER_GPUI_SCENARIO_MEDIA";

/// One named start state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenario {
    /// The id of the catalogue, for example `shell-empty`.
    pub id: &'static str,
    pub theme: ThemeName,
    pub seed: Seed,
    /// The tile size the gallery starts with.
    pub tile_size: u32,
}

/// Every scenario the GPUI interface can start in, on a fresh profile.
pub const SCENARIOS: [Scenario; 5] = [
    Scenario {
        id: "shell-empty",
        theme: ThemeName::Dark,
        seed: Seed::Empty,
        tile_size: DEFAULT_TILE_SIZE,
    },
    Scenario {
        id: "shell-empty-light",
        theme: ThemeName::Light,
        seed: Seed::Empty,
        tile_size: DEFAULT_TILE_SIZE,
    },
    Scenario {
        id: "gallery-default",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
    },
    Scenario {
        id: "gallery-tiles-small",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: TILE_SIZE_MIN,
    },
    Scenario {
        id: "gallery-tiles-large",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: TILE_SIZE_MAX,
    },
];

/// The scenario with this id. Ids are matched whole and case-sensitively.
pub fn find(id: &str) -> Option<Scenario> {
    SCENARIOS.into_iter().find(|scenario| scenario.id == id)
}

/// The board of every shared control in both themes (epic E05, [`crate::kit`]). It
/// has no Tauri counterpart, so it is no comparison scenario: it is not in
/// [`SCENARIOS`], the catalogue of `epics.json` or `e2e/compare/scenarios`. The E2E
/// build starts on it with `--scenario kit` for review.
pub const KIT: &str = "kit";

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, path::PathBuf};

    use super::{find, Seed, SCENARIOS};
    use crate::{
        runtime::{DEFAULT_TILE_SIZE, TILE_SIZE_MAX, TILE_SIZE_MIN},
        theme::ThemeName,
    };

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    #[test]
    fn finds_a_scenario_by_its_whole_id() {
        assert_eq!(find("shell-empty").map(|s| s.theme), Some(ThemeName::Dark));
        assert_eq!(
            find("shell-empty-light").map(|s| s.theme),
            Some(ThemeName::Light)
        );
        for unknown in [
            "",
            "shell",
            "shell-empty ",
            "Shell-Empty",
            "shell-empty-dark",
        ] {
            assert_eq!(find(unknown), None, "{unknown:?}");
        }
    }

    #[test]
    fn ids_are_unique_and_safe_as_file_names() {
        let ids: BTreeSet<_> = SCENARIOS.iter().map(|scenario| scenario.id).collect();
        assert_eq!(ids.len(), SCENARIOS.len());
        for id in ids {
            assert!(
                !id.is_empty()
                    && id.split('-').all(|part| !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())),
                "{id:?} is not lower-case words joined by single hyphens"
            );
        }
    }

    #[test]
    fn every_scenario_is_in_the_plan_catalogue() {
        let manifest = repository_root().join("docs/plans/gpui-port/epics.json");
        let text = std::fs::read_to_string(&manifest).expect("read epics.json");
        let json: serde_json::Value = serde_json::from_str(&text).expect("epics.json is JSON");
        let catalogue = json["scenarios"]
            .as_object()
            .expect("epics.json has a scenarios object");

        for scenario in SCENARIOS {
            assert!(
                catalogue.contains_key(scenario.id),
                "{} is missing from the scenarios of epics.json",
                scenario.id
            );
        }
    }

    #[test]
    fn every_scenario_has_its_tauri_steps() {
        let steps = repository_root().join("e2e/compare/scenarios");

        for scenario in SCENARIOS {
            let file = steps.join(format!("{}.js", scenario.id));
            assert!(file.is_file(), "{} is missing", file.display());
        }
    }

    /// The value of `field: "value"` in a scenario file of `e2e/compare/scenarios`.
    fn js_string_field(source: &str, field: &str) -> Option<String> {
        source.lines().find_map(|line| {
            let rest = line
                .trim()
                .strip_prefix(field)?
                .trim_start()
                .strip_prefix(':')?;
            let value = rest.trim().trim_end_matches(',').trim();
            Some(value.strip_prefix('"')?.strip_suffix('"')?.to_owned())
        })
    }

    #[test]
    fn every_scenario_starts_in_the_theme_of_its_tauri_steps() {
        let steps = repository_root().join("e2e/compare/scenarios");

        for scenario in SCENARIOS {
            let file = steps.join(format!("{}.js", scenario.id));
            let source = std::fs::read_to_string(&file).expect("read the Tauri steps");
            assert_eq!(
                js_string_field(&source, "id").as_deref(),
                Some(scenario.id),
                "{}",
                file.display()
            );
            assert_eq!(
                js_string_field(&source, "theme").as_deref(),
                Some(scenario.theme.css_name()),
                "{}: theme of the Tauri side",
                scenario.id
            );
            assert_eq!(
                js_string_field(&source, "seed").as_deref(),
                Some(scenario.seed.name()),
                "{}: seed of the Tauri side",
                scenario.id
            );
        }
    }

    #[test]
    fn the_gallery_scenarios_show_the_control_seed_at_three_tile_sizes() {
        // Parity scenarios of E09.
        for (id, tile_size) in [
            ("gallery-default", DEFAULT_TILE_SIZE),
            ("gallery-tiles-small", TILE_SIZE_MIN),
            ("gallery-tiles-large", TILE_SIZE_MAX),
        ] {
            let scenario = find(id).unwrap_or_else(|| panic!("{id} is a scenario"));
            assert_eq!(scenario.seed, Seed::Control, "{id}");
            assert_eq!(scenario.tile_size, tile_size, "{id}");
            assert_eq!(scenario.theme, ThemeName::Dark, "{id}");
        }
        for scenario in SCENARIOS {
            if scenario.seed == Seed::Empty {
                assert_eq!(scenario.tile_size, DEFAULT_TILE_SIZE, "{}", scenario.id);
            }
        }
        assert_eq!(
            super::SCENARIO_MEDIA_VARIABLE,
            "MEDIATAGGER_GPUI_SCENARIO_MEDIA"
        );
    }

    #[test]
    fn every_tauri_scenario_has_a_gpui_start_state() {
        let steps = repository_root().join("e2e/compare/scenarios");
        let mut seen = 0;

        for entry in std::fs::read_dir(&steps).expect("read e2e/compare/scenarios") {
            let path = entry.expect("directory entry").path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("js") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("a UTF-8 file name");
            assert!(
                find(id).is_some(),
                "{} has Tauri steps and no GPUI start state",
                id
            );
            seen += 1;
        }
        assert!(seen >= SCENARIOS.len(), "only {seen} Tauri scenario files");
    }

    #[test]
    fn the_kit_board_is_no_comparison_scenario() {
        assert_eq!(find(super::KIT), None);
        let steps = repository_root().join("e2e/compare/scenarios");
        assert!(!steps.join(format!("{}.js", super::KIT)).exists());
        let manifest = repository_root().join("docs/plans/gpui-port/epics.json");
        let text = std::fs::read_to_string(&manifest).expect("read epics.json");
        let json: serde_json::Value = serde_json::from_str(&text).expect("epics.json is JSON");
        assert!(json["scenarios"].get(super::KIT).is_none());
    }

    #[test]
    fn the_harness_starts_with_the_two_empty_shell_scenarios() {
        // Acceptance criterion of E01: `compare:ui shell-empty` and `shell-empty-light`.
        assert_eq!(find("shell-empty").map(|s| s.theme), Some(ThemeName::Dark));
        assert_eq!(
            find("shell-empty-light").map(|s| s.theme),
            Some(ThemeName::Light)
        );
    }

    #[test]
    fn every_parity_scenario_of_a_started_epic_has_a_gpui_start_state() {
        let manifest = repository_root().join("docs/plans/gpui-port/epics.json");
        let text = std::fs::read_to_string(&manifest).expect("read epics.json");
        let json: serde_json::Value = serde_json::from_str(&text).expect("epics.json is JSON");
        let epics = json["epics"]
            .as_array()
            .expect("epics.json has an epics list");
        let mut checked = 0;

        for epic in epics {
            // Only an epic that has started owes its scenarios; a later one adds its own.
            let status = epic["status"].as_str().unwrap_or("todo");
            if !(status == "in progress" || status.starts_with("done")) {
                continue;
            }
            for id in epic["parity"].as_array().into_iter().flatten() {
                let id = id.as_str().expect("a parity entry is an id");
                assert!(
                    find(id).is_some(),
                    "{} ({status}) compares {id}, which has no GPUI start state",
                    epic["id"]
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 1,
            "no parity scenario of a started epic was checked"
        );
    }
}
