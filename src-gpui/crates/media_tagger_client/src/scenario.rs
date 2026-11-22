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
    backend::MediaKind,
    bulk::DockTab,
    gallery::{Interaction, SelectionInput},
    library::AssetQuery,
    runtime::{DEFAULT_TILE_SIZE, TILE_SIZE_MAX, TILE_SIZE_MIN},
    search::{tag_list::TagListInput, SearchCommand},
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

/// One click on a tile in selection mode, as a scenario's Tauri steps make it
/// (`click` and `modclick` on `button[data-asset-index="<index>"]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioClick {
    /// The global index of the tile.
    pub index: usize,
    /// Control is held.
    pub ctrl: bool,
    /// Shift is held.
    pub shift: bool,
}

/// Tags a scenario gives the tile at `index` of the unfiltered library before the
/// window opens; the Tauri steps add them through the bulk dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioTags {
    pub index: usize,
    pub tags: &'static [&'static str],
}

/// The search header a scenario starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioSearch {
    /// The search field's text.
    pub input: &'static str,
    /// A media kind chosen in the segmented control; choosing one submits the draft.
    pub kind: Option<MediaKind>,
    /// The field has the focus, with the caret at the end: its suggestions open.
    pub focus: bool,
}

impl ScenarioSearch {
    /// The commands that make this header: the typed text, then the media kind.
    pub fn commands(&self) -> Vec<SearchCommand> {
        let mut commands = vec![SearchCommand::Change(self.input.to_owned())];
        if let Some(kind) = self.kind {
            commands.push(SearchCommand::ApplyKind(Some(kind)));
        }
        commands
    }
}

/// The tag list a scenario opens, with tags included and excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioTagList {
    /// Tags included: the Tauri steps click them once.
    pub include: &'static [&'static str],
    /// Tags excluded: the Tauri steps double-click them.
    pub exclude: &'static [&'static str],
}

impl ScenarioTagList {
    /// The inputs that open the list and make the choices. A double click excludes an
    /// inactive tag and a second one includes it, so no click waits for its timer.
    pub fn inputs(&self) -> Vec<TagListInput> {
        let mut inputs = vec![TagListInput::Open];
        for tag in self.include {
            for _ in 0..2 {
                inputs.push(TagListInput::DoubleClick {
                    tag: (*tag).to_owned(),
                });
            }
        }
        for tag in self.exclude {
            inputs.push(TagListInput::DoubleClick {
                tag: (*tag).to_owned(),
            });
        }
        inputs
    }

    /// The tag the last Tauri step clicks: a click gives its button the focus, so the
    /// dialog starts with the focus there instead of in the filter.
    pub fn focused(&self) -> Option<&'static str> {
        self.exclude.last().or_else(|| self.include.last()).copied()
    }
}

/// What a scenario does in the bulk dock once its clicks are made, as its Tauri steps
/// do: a tag added with Enter, a tab chosen, a group key typed, the larger order view
/// opened. No field gets the focus by it: a capture of the Tauri window on its private
/// display shows neither a caret nor a focus ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScenarioDock {
    /// Typed into the tag field and added with Enter.
    pub tag: Option<&'static str>,
    pub tab: DockTab,
    /// Typed into the group key field.
    pub group_key: Option<&'static str>,
    /// "Sort in larger view" is pressed once the inline board is there.
    pub order_modal: bool,
}

/// `bulk-dock-tags`: `holiday` added to the three selected tiles.
pub const BULK_DOCK_TAGS: ScenarioDock = ScenarioDock {
    tag: Some("holiday"),
    tab: DockTab::Tags,
    group_key: None,
    order_modal: false,
};

/// `bulk-dock-group`: the group tab with the key `trip` typed, which shows the inline
/// order board of the three selected tiles.
pub const BULK_DOCK_GROUP: ScenarioDock = ScenarioDock {
    tag: None,
    tab: DockTab::Group,
    group_key: Some("trip"),
    order_modal: false,
};

/// `group-order-modal`: [`BULK_DOCK_GROUP`] with the larger order view open.
pub const GROUP_ORDER_MODAL: ScenarioDock = ScenarioDock {
    order_modal: true,
    ..BULK_DOCK_GROUP
};

/// `tag-list-modal`: the tags of [`SEARCH_SUGGESTION_TAGS`], the tag list open with
/// `camera` included and `cat` excluded.
pub const TAG_LIST_MODAL: ScenarioTagList = ScenarioTagList {
    include: &["camera"],
    exclude: &["cat"],
};

/// `search-suggestions`: the first tile tagged `camera`, `car` and `cat`, and `ca`
/// typed into the focused search field.
pub const SEARCH_SUGGESTION_TAGS: [ScenarioTags; 1] = [ScenarioTags {
    index: 0,
    tags: &["camera", "car", "cat"],
}];

/// One named start state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scenario {
    /// The id of the catalogue, for example `shell-empty`.
    pub id: &'static str,
    pub theme: ThemeName,
    pub seed: Seed,
    /// The tile size the gallery starts with.
    pub tile_size: u32,
    /// The clicks the gallery starts with, in selection mode; without clicks
    /// selection mode stays off. They are made once their tiles are loaded
    /// ([`selection_inputs`]). A Shift click reads its range in the background, so
    /// only the last click may be one.
    pub selection: &'static [ScenarioClick],
    /// Tags given to tiles before the window opens.
    pub tags: &'static [ScenarioTags],
    /// The search header, if the scenario sets one.
    pub search: Option<ScenarioSearch>,
    /// The tag list, open with these choices, if the scenario opens it.
    pub tag_list: Option<ScenarioTagList>,
    /// What the scenario does in the bulk dock after its clicks.
    pub dock: Option<ScenarioDock>,
}

/// `gallery-selection`: a click on the second tile, a Ctrl-click on the first and a
/// Shift-click on the third, which selects the range from the first (the anchor of
/// the Ctrl-click): the first three tiles.
pub const GALLERY_SELECTION_CLICKS: [ScenarioClick; 3] = [
    ScenarioClick {
        index: 1,
        ctrl: false,
        shift: false,
    },
    ScenarioClick {
        index: 0,
        ctrl: true,
        shift: false,
    },
    ScenarioClick {
        index: 2,
        ctrl: false,
        shift: true,
    },
];

/// The inputs that make `clicks` in selection mode: the mode toggle, then one click
/// per entry on the asset its tile shows. `None` until the query has a session and
/// the row of every clicked tile is loaded. No clicks: no inputs.
pub fn selection_inputs(
    clicks: &[ScenarioClick],
    query: &AssetQuery,
) -> Option<Vec<SelectionInput>> {
    if clicks.is_empty() {
        return Some(Vec::new());
    }
    if query.session_id().is_none() || query.query_pending() {
        return None;
    }
    let mut inputs = vec![SelectionInput::ToggleMode];
    for click in clicks {
        let asset = query.asset_at(click.index)?;
        inputs.push(SelectionInput::Interact(Interaction::Click {
            asset_id: asset.id,
            asset_index: click.index,
            ctrl_like: click.ctrl,
            shift: click.shift,
        }));
    }
    Some(inputs)
}

/// Every scenario the GPUI interface can start in, on a fresh profile.
pub const SCENARIOS: [Scenario; 12] = [
    Scenario {
        id: "shell-empty",
        theme: ThemeName::Dark,
        seed: Seed::Empty,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "shell-empty-light",
        theme: ThemeName::Light,
        seed: Seed::Empty,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "gallery-default",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "gallery-tiles-small",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: TILE_SIZE_MIN,
        selection: &[],
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "gallery-tiles-large",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: TILE_SIZE_MAX,
        selection: &[],
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "gallery-selection",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &GALLERY_SELECTION_CLICKS,
        tags: &[],
        search: None,
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "search-suggestions",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &SEARCH_SUGGESTION_TAGS,
        search: Some(ScenarioSearch {
            input: "ca",
            kind: None,
            focus: true,
        }),
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "search-applied",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &[],
        search: Some(ScenarioSearch {
            input: "-cat",
            kind: Some(MediaKind::Image),
            focus: false,
        }),
        tag_list: None,
        dock: None,
    },
    Scenario {
        id: "tag-list-modal",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &[],
        tags: &SEARCH_SUGGESTION_TAGS,
        search: None,
        tag_list: Some(TAG_LIST_MODAL),
        dock: None,
    },
    Scenario {
        id: "bulk-dock-tags",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &GALLERY_SELECTION_CLICKS,
        tags: &[],
        search: None,
        tag_list: None,
        dock: Some(BULK_DOCK_TAGS),
    },
    Scenario {
        id: "bulk-dock-group",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &GALLERY_SELECTION_CLICKS,
        tags: &[],
        search: None,
        tag_list: None,
        dock: Some(BULK_DOCK_GROUP),
    },
    Scenario {
        id: "group-order-modal",
        theme: ThemeName::Dark,
        seed: Seed::Control,
        tile_size: DEFAULT_TILE_SIZE,
        selection: &GALLERY_SELECTION_CLICKS,
        tags: &[],
        search: None,
        tag_list: None,
        dock: Some(GROUP_ORDER_MODAL),
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
    fn the_selection_scenario_makes_the_clicks_of_its_tauri_steps() {
        // Parity scenario of E11.
        let scenario = find("gallery-selection").expect("gallery-selection is a scenario");
        assert_eq!(scenario.seed, Seed::Control);
        assert_eq!(scenario.tile_size, DEFAULT_TILE_SIZE);
        assert_eq!(scenario.selection.len(), 3);
        // Only the last click may read a range.
        let (last, earlier) = scenario.selection.split_last().expect("clicks");
        assert!(last.shift && earlier.iter().all(|click| !click.shift));

        assert_steps_in_order("gallery-selection", &selection_steps(scenario.selection));
        for scenario in SCENARIOS {
            if scenario.id != "gallery-selection" && scenario.dock.is_none() {
                assert!(scenario.selection.is_empty(), "{}", scenario.id);
            }
        }
    }

    /// The Tauri steps that turn selection mode on and make `clicks`.
    fn selection_steps(clicks: &[super::ScenarioClick]) -> Vec<String> {
        let mut steps = vec![r#"["click", 'button[aria-label="Enable bulk actions"]']"#.to_owned()];
        for click in clicks {
            let tile = format!(r#"'button[data-asset-index="{}"]'"#, click.index);
            steps.push(match (click.ctrl, click.shift) {
                (false, false) => format!(r#"["click", {tile}]"#),
                (true, false) => format!(r#"["modclick", "Control", {tile}]"#),
                (false, true) => format!(r#"["modclick", "Shift", {tile}]"#),
                (true, true) => format!(r#"["modclick", "Control+Shift", {tile}]"#),
            });
        }
        steps
    }

    /// Every step is in the Tauri steps of scenario `id`, in this order.
    fn assert_steps_in_order(id: &str, steps: &[String]) {
        let file = repository_root().join(format!("e2e/compare/scenarios/{id}.js"));
        let source = std::fs::read_to_string(&file).expect("read the Tauri steps");
        let mut from = 0;
        for step in steps {
            let at = source[from..].find(step.as_str()).unwrap_or_else(|| {
                panic!("{id}: {step} is missing, or out of order, in the Tauri steps")
            });
            from += at + step.len();
        }
    }

    #[test]
    fn the_dock_scenarios_do_in_the_dock_what_their_tauri_steps_do() {
        // Parity scenarios of E14.
        use crate::bulk::DockTab;
        for (id, tag, tab, group_key, order_modal) in [
            (
                "bulk-dock-tags",
                Some("holiday"),
                DockTab::Tags,
                None,
                false,
            ),
            ("bulk-dock-group", None, DockTab::Group, Some("trip"), false),
            (
                "group-order-modal",
                None,
                DockTab::Group,
                Some("trip"),
                true,
            ),
        ] {
            let scenario = find(id).unwrap_or_else(|| panic!("{id} is a scenario"));
            assert_eq!(scenario.seed, Seed::Control, "{id}");
            assert_eq!(
                scenario.selection,
                &super::GALLERY_SELECTION_CLICKS[..],
                "{id}"
            );
            let dock = scenario
                .dock
                .unwrap_or_else(|| panic!("{id} uses the dock"));
            assert_eq!(
                (dock.tag, dock.tab, dock.group_key, dock.order_modal),
                (tag, tab, group_key, order_modal),
                "{id}"
            );

            // The clicks, then the dock's steps, in the order the GPUI side makes them.
            let mut steps = selection_steps(scenario.selection);
            if let Some(tag) = dock.tag {
                steps.push(format!(r##"["fill", "#bulk-tag-draft-input", "{tag}"]"##));
                steps.push(r#"["keys", "Enter"]"#.to_owned());
            }
            if dock.tab == DockTab::Group {
                steps.push(r#"["click", '[data-testid="bulk-tab-group"]']"#.to_owned());
            }
            if let Some(key) = dock.group_key {
                steps.push(format!(r##"["fill", "#bulk-group-key-input", "{key}"]"##));
                steps.push(r#"["wait", '[data-testid="bulk-inline-order-grid"]']"#.to_owned());
            }
            if dock.order_modal {
                steps.push(r#"["click", '[data-testid="bulk-open-order-modal"]']"#.to_owned());
                steps.push(r#"["wait", '[data-testid="bulk-order-modal"]']"#.to_owned());
            }
            assert_steps_in_order(id, &steps);
        }
        for scenario in SCENARIOS {
            if !["bulk-dock-tags", "bulk-dock-group", "group-order-modal"].contains(&scenario.id) {
                assert!(scenario.dock.is_none(), "{}", scenario.id);
            }
        }
    }

    #[test]
    fn the_selection_clicks_wait_for_the_rows_of_their_tiles() {
        use std::sync::Arc;

        use media_tagger_core::models::{AssetSummary, StartAssetQueryResult};

        use crate::{
            backend::{Backend, FakeBackend},
            exec::{Executor, ManualExecutor},
            gallery::{Interaction, SelectionInput},
            library::{AssetQuery, QueryConfig, QueryInput, QueryRunner},
            model::Driver,
        };

        fn row(id: i64) -> AssetSummary {
            AssetSummary {
                id,
                file_name: format!("{id}.png"),
                preview_path: None,
                kind: "image".into(),
                modified_at: 0,
                width: None,
                height: None,
                duration_ms: None,
                thumb_version: format!("{id:016x}"),
                is_favorite: false,
                media_group_key: None,
                media_group_order: None,
            }
        }
        let backend = Arc::new(FakeBackend::new());
        backend.on_start_asset_query(|_| {
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: 7,
                offset: 0,
                // The IDs are not the indexes plus one.
                items: [40, 10, 30, 20, 50, 60, 70].into_iter().map(row).collect(),
            })
        });
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = Driver::new(
            AssetQuery::new(QueryConfig::default()),
            QueryRunner::new(backend as Arc<dyn Backend>),
            executor.clone() as Arc<dyn Executor>,
        );
        let clicks = &super::GALLERY_SELECTION_CLICKS;

        assert_eq!(
            super::selection_inputs(&[], driver.model()),
            Some(Vec::new())
        );
        assert!(super::selection_inputs(clicks, driver.model()).is_none());
        driver.handle(QueryInput::Refresh);
        assert!(super::selection_inputs(clicks, driver.model()).is_none());
        executor.run_all();
        driver.pump();

        let inputs = super::selection_inputs(clicks, driver.model()).expect("the rows are loaded");
        let clicked: Vec<_> = inputs
            .iter()
            .map(|input| match input {
                SelectionInput::ToggleMode => None,
                SelectionInput::Interact(Interaction::Click {
                    asset_id,
                    asset_index,
                    ctrl_like,
                    shift,
                }) => Some((*asset_id, *asset_index, *ctrl_like, *shift)),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            clicked,
            [
                None,
                Some((10, 1, false, false)),
                Some((40, 0, true, false)),
                Some((30, 2, false, true))
            ]
        );
    }

    #[test]
    fn the_search_scenarios_type_what_their_tauri_steps_type() {
        // Parity scenarios of E12.
        use crate::{backend::MediaKind, search::SearchCommand};
        let suggestions = find("search-suggestions").expect("a scenario");
        let search = suggestions.search.expect("a search header");
        assert!(search.focus && search.kind.is_none());
        assert_eq!(search.commands(), [SearchCommand::Change("ca".into())]);
        assert_eq!(suggestions.tags, &super::SEARCH_SUGGESTION_TAGS[..]);
        let applied = find("search-applied").expect("a scenario");
        let search = applied.search.expect("a search header");
        assert!(!search.focus && applied.tags.is_empty());
        assert_eq!(
            search.commands(),
            [
                SearchCommand::Change("-cat".into()),
                SearchCommand::ApplyKind(Some(MediaKind::Image))
            ]
        );

        let steps = repository_root().join("e2e/compare/scenarios");
        let source = std::fs::read_to_string(steps.join("search-suggestions.js"))
            .expect("read the Tauri steps");
        for tag in super::SEARCH_SUGGESTION_TAGS[0].tags {
            assert!(
                source.contains(&format!(r##"["fill", "#bulk-tag-draft-input", "{tag}"]"##)),
                "{tag} is not added on the Tauri side"
            );
        }
        assert!(source.contains(r#"["click", 'button[data-asset-index="0"]']"#));
        assert!(source.contains(r#"["fill", ".filter-input", "ca"]"#));
        let source =
            std::fs::read_to_string(steps.join("search-applied.js")).expect("read the Tauri steps");
        assert!(source.contains(r#"["fill", ".filter-input", "-cat"]"#));
        assert!(source.contains(r#"input[value="image"]"#));
        for scenario in SCENARIOS {
            if !scenario.id.starts_with("search-") {
                assert!(scenario.search.is_none(), "{}", scenario.id);
            }
            if !scenario.id.starts_with("search-") && scenario.id != "tag-list-modal" {
                assert!(scenario.tags.is_empty(), "{}", scenario.id);
            }
        }
    }

    #[test]
    fn the_tag_list_scenario_makes_the_choices_of_its_tauri_steps() {
        // Parity scenario of E13.
        use crate::search::tag_list::{TagListInput, TagSelectionMode, TagSelections};
        let scenario = find("tag-list-modal").expect("a scenario");
        assert_eq!(scenario.seed, Seed::Control);
        assert_eq!(scenario.tags, &super::SEARCH_SUGGESTION_TAGS[..]);
        let tag_list = scenario.tag_list.expect("the tag list is open");
        let inputs = tag_list.inputs();
        assert!(matches!(inputs[0], TagListInput::Open));
        let mut selections = TagSelections::new();
        for input in &inputs[1..] {
            let TagListInput::DoubleClick { tag } = input else {
                panic!("{input:?}");
            };
            selections = selections.apply_double(tag);
        }
        assert_eq!(selections.mode("camera"), Some(TagSelectionMode::Include));
        assert_eq!(selections.mode("cat"), Some(TagSelectionMode::Exclude));
        assert_eq!(selections.included_count() + selections.excluded_count(), 2);
        // The double click on cat is the last step: its button has the focus.
        assert_eq!(tag_list.focused(), Some("cat"));

        let source = std::fs::read_to_string(
            repository_root().join("e2e/compare/scenarios/tag-list-modal.js"),
        )
        .expect("read the Tauri steps");
        for tag in super::SEARCH_SUGGESTION_TAGS[0].tags {
            assert!(
                source.contains(&format!(r##"["fill", "#bulk-tag-draft-input", "{tag}"]"##)),
                "{tag} is not added on the Tauri side"
            );
        }
        for tag in tag_list.include {
            assert!(source.contains(&format!(
                r#"["click", 'button[aria-label="{tag}, state: inactive"]']"#
            )));
        }
        for tag in tag_list.exclude {
            assert!(source.contains(&format!(
                r#"["dblclick", 'button[aria-label="{tag}, state: inactive"]']"#
            )));
        }
        let focused = tag_list.focused().expect("a clicked tag");
        let last_click = source
            .rfind(r#"click", 'button[aria-label=""#)
            .expect("a click on a tag");
        assert!(
            source[last_click..].starts_with(&format!(
                r#"click", 'button[aria-label="{focused}, state: inactive"]'"#
            )),
            "the last Tauri click is not on {focused}"
        );
        for scenario in SCENARIOS {
            if scenario.id != "tag-list-modal" {
                assert!(scenario.tag_list.is_none(), "{}", scenario.id);
            }
        }
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
