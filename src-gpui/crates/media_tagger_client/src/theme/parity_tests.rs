//! The token constants against `src/styles.css`: a value changed on one side only, or
//! a token the stylesheet gains without a Rust counterpart, fails here.

use std::{collections::BTreeSet, path::Path, time::Duration};

use super::{
    css::{self, Rule, Scope},
    Rgba, ThemeName, ThemeTokens, Typography, DARK, LIGHT, MOTION, RADII, TYPOGRAPHY,
};

const STYLES_CSS: &str = include_str!("../../../../../src/styles.css");

const THEME_PLUGIN: &str = "@plugin \"daisyui/theme\"";

/// Custom properties of the theme blocks and `:root` that are not tokens of this
/// module, and why.
const NOT_CARRIED: [(&str, &str); 11] = [
    (
        "--p",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--pc",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--s",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--bc",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--b1",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--b2",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--b3",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--su",
        "oklch channels for the native video player bridge (E21)",
    ),
    (
        "--select-chevron",
        "an icon, drawn by the select control (E05)",
    ),
    ("--depth", "DaisyUI depth effect, off (checked to be 0)"),
    ("--noise", "DaisyUI noise effect, off (checked to be 0)"),
];

fn find<'a>(rules: &'a [Rule], prelude: &str) -> Result<&'a Rule, String> {
    rules
        .iter()
        .find(|rule| rule.prelude == prelude)
        .ok_or_else(|| format!("styles.css has no rule {prelude}"))
}

fn theme_block(rules: &[Rule], theme: ThemeName) -> Result<&Rule, String> {
    let name = format!("\"{}\"", theme.css_name());
    rules
        .iter()
        .find(|rule| rule.prelude == THEME_PLUGIN && rule.get("name") == Some(name.as_str()))
        .ok_or_else(|| format!("styles.css has no DaisyUI theme {name}"))
}

fn data_theme_selector(theme: ThemeName) -> String {
    format!("[data-theme=\"{}\"]", theme.css_name())
}

/// The custom properties of `theme` in cascade order: `@theme`, the DaisyUI theme,
/// `:root`, `[data-theme]`.
fn scope(rules: &[Rule], theme: ThemeName) -> Result<Scope, String> {
    let sources = [
        find(rules, "@theme")?,
        theme_block(rules, theme)?,
        find(rules, ":root")?,
        find(rules, &data_theme_selector(theme))?,
    ];
    Ok(sources
        .into_iter()
        .flat_map(|rule| rule.declarations.iter())
        .filter(|(name, _)| name.starts_with("--"))
        .cloned()
        .collect())
}

fn value<'a>(scope: &'a Scope, name: &str) -> Result<&'a str, String> {
    scope
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} is not defined"))
}

/// Every difference between `css` and the constants `themes`, `RADII` and `MOTION`.
fn parity_problems(css: &str, themes: [&ThemeTokens; 2]) -> Vec<String> {
    let rules = css::rules(css);
    let mut problems = Vec::new();
    for tokens in themes {
        let theme = tokens.name.css_name();
        if let Err(problem) = theme_problems(&rules, tokens, &mut problems) {
            problems.push(format!("{theme}: {problem}"));
        }
    }
    if let Err(problem) = motion_problems(&rules, &mut problems) {
        problems.push(problem);
    }
    problems
}

fn theme_problems(
    rules: &[Rule],
    tokens: &ThemeTokens,
    problems: &mut Vec<String>,
) -> Result<(), String> {
    let theme = tokens.name.css_name();
    let scope = scope(rules, tokens.name)?;
    let mut carried = BTreeSet::new();

    for (name, expected) in tokens.colors.css_entries() {
        carried.insert(name);
        match value(&scope, name).and_then(|css| css::color(&scope, css)) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => problems.push(format!(
                "{theme} {name}: styles.css {actual:?}, Rust {expected:?}"
            )),
            Err(error) => problems.push(format!("{theme} {name}: {error}")),
        }
    }
    for (name, expected) in tokens.shadows.css_entries() {
        carried.insert(name);
        match value(&scope, name).and_then(|css| css::shadows(&scope, css)) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => problems.push(format!(
                "{theme} {name}: styles.css {actual:?}, Rust {expected:?}"
            )),
            Err(error) => problems.push(format!("{theme} {name}: {error}")),
        }
    }
    for (name, expected) in RADII.css_entries() {
        carried.insert(name);
        match value(&scope, name).and_then(|css| css::px(&scope, css)) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => problems.push(format!(
                "{theme} {name}: styles.css {actual}px, Rust {expected}px"
            )),
            Err(error) => problems.push(format!("{theme} {name}: {error}")),
        }
    }
    carried.extend(MOTION_TOKENS);

    let block = theme_block(rules, tokens.name)?;
    for (name, expected) in [("--depth", "0"), ("--noise", "0"), ("color-scheme", theme)] {
        if block.get(name) != Some(expected) {
            problems.push(format!(
                "{theme} {name} is {:?}, expected {expected}",
                block.get(name)
            ));
        }
    }

    let ignored: BTreeSet<&str> = NOT_CARRIED.iter().map(|(name, _)| *name).collect();
    let declared = [
        block,
        find(rules, ":root")?,
        find(rules, &data_theme_selector(tokens.name))?,
    ];
    for (name, _) in declared.iter().flat_map(|rule| rule.declarations.iter()) {
        if name.starts_with("--")
            && !carried.contains(name.as_str())
            && !ignored.contains(name.as_str())
        {
            problems.push(format!(
                "{theme} {name} is in styles.css but not in media_tagger_client::theme"
            ));
        }
    }
    Ok(())
}

const MOTION_TOKENS: [&str; 4] = [
    "--motion-feedback",
    "--motion-entrance",
    "--motion-exit",
    "--motion-ease",
];

fn motion_problems(rules: &[Rule], problems: &mut Vec<String>) -> Result<(), String> {
    let root: Scope = find(rules, ":root")?.declarations.iter().cloned().collect();
    let lightbox: Scope = find(rules, "[data-lightbox-backdrop]")?
        .declarations
        .iter()
        .cloned()
        .collect();
    let durations: [(&Scope, &str, Duration); 5] = [
        (&root, "--motion-feedback", MOTION.feedback),
        (&root, "--motion-entrance", MOTION.entrance),
        (&root, "--motion-exit", MOTION.exit),
        (&lightbox, "--modal-entrance", MOTION.lightbox_entrance),
        (&lightbox, "--modal-exit", MOTION.lightbox_exit),
    ];
    for (scope, name, expected) in durations {
        match value(scope, name).and_then(|css| css::duration(scope, css)) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => {
                problems.push(format!("{name}: styles.css {actual:?}, Rust {expected:?}"))
            }
            Err(error) => problems.push(format!("{name}: {error}")),
        }
    }
    match value(&root, "--motion-ease").and_then(|css| css::cubic_bezier(&root, css)) {
        Ok(actual) if actual == MOTION.ease => {}
        Ok(actual) => problems.push(format!(
            "--motion-ease: styles.css {actual:?}, Rust {:?}",
            MOTION.ease
        )),
        Err(error) => problems.push(format!("--motion-ease: {error}")),
    }
    Ok(())
}

#[test]
fn the_constants_match_styles_css() {
    let problems = parity_problems(STYLES_CSS, [&LIGHT, &DARK]);

    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_colour_changed_only_in_styles_css_fails() {
    let changed = STYLES_CSS.replacen(
        "--color-base-200: #141618;",
        "--color-base-200: #151618;",
        1,
    );
    assert_ne!(changed, STYLES_CSS, "the dark base-200 declaration exists");

    let problems = parity_problems(&changed, [&LIGHT, &DARK]);

    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("dark --color-base-200:")),
        "{problems:#?}"
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("dark --bg-0:")),
        "the aliases follow: {problems:#?}"
    );
    assert!(
        problems.iter().all(|problem| problem.starts_with("dark ")),
        "{problems:#?}"
    );
}

#[test]
fn a_colour_changed_only_in_rust_fails() {
    let mut light = LIGHT;
    light.colors.error = Rgba::hex(0xb13c39);

    let problems = parity_problems(STYLES_CSS, [&light, &DARK]);

    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(
        problems[0].starts_with("light --color-error:"),
        "{problems:#?}"
    );
}

#[test]
fn a_shadow_or_motion_change_on_one_side_fails() {
    let changed = STYLES_CSS
        .replacen(
            "--shadow-modal: 0 16px 48px rgb(0 0 0 / 28%);",
            "--shadow-modal: 0 16px 48px rgb(0 0 0 / 30%);",
            1,
        )
        .replacen("--motion-entrance: 150ms;", "--motion-entrance: 160ms;", 1);
    assert_ne!(changed, STYLES_CSS);

    let problems = parity_problems(&changed, [&LIGHT, &DARK]);

    for expected in [
        "light --shadow-modal:",
        "dark --shadow-modal:",
        "--motion-entrance:",
    ] {
        assert!(
            problems.iter().any(|problem| problem.starts_with(expected)),
            "{expected} in {problems:#?}"
        );
    }
}

#[test]
fn a_new_token_in_styles_css_must_be_carried() {
    let changed = STYLES_CSS.replacen(
        "[data-theme=\"dark\"] {",
        "[data-theme=\"dark\"] {\n  --color-new-token: #123456;",
        1,
    );
    assert_ne!(changed, STYLES_CSS);

    let problems = parity_problems(&changed, [&LIGHT, &DARK]);

    assert_eq!(
        problems,
        ["dark --color-new-token is in styles.css but not in media_tagger_client::theme"]
    );
}

#[test]
fn the_light_and_dark_themes_differ_where_the_stylesheet_does() {
    assert_ne!(LIGHT.colors.base_200, DARK.colors.base_200);
    assert_eq!(LIGHT.colors.media_gif_bg, DARK.colors.media_gif_bg);
    assert_eq!(
        DARK.colors.bg_1, DARK.colors.base_200,
        "the application background"
    );
    assert!(!LIGHT.colors.gallery_group_bg.is_opaque());
    assert!(DARK.colors.gallery_group_bg.is_opaque());
}

fn font_list(value: &str) -> Vec<String> {
    css::split_top_level(value, ',')
        .into_iter()
        .map(|family| {
            family
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_owned()
        })
        .collect()
}

fn typography_problems(css: &str, typography: &Typography) -> Vec<String> {
    let rules = css::rules(css);
    let mut problems = Vec::new();
    let mut check = |what: &str, actual: Result<String, String>, expected: String| match actual {
        Ok(actual) if actual == expected => {}
        Ok(actual) => problems.push(format!("{what}: styles.css {actual}, Rust {expected}")),
        Err(error) => problems.push(format!("{what}: {error}")),
    };
    let declaration = |prelude: &str, property: &str| -> Result<String, String> {
        find(&rules, prelude)?
            .get(property)
            .map(str::to_owned)
            .ok_or_else(|| format!("{prelude} has no {property}"))
    };
    let px = |size: f32| format!("{size}px");

    for (property, step) in [
        ("--text-sm", typography.sm),
        ("--text-base", typography.base),
        ("--text-lg", typography.lg),
        ("--text-2xl", typography.xxl),
    ] {
        check(property, declaration("@theme", property), px(step.size));
    }
    check(
        "--font-mono",
        declaration("@theme", "--font-mono").map(|value| font_list(&value).join(",")),
        typography.mono.join(","),
    );
    check(
        ":root font-family",
        declaration(":root", "font-family").map(|value| font_list(&value).join(",")),
        typography.sans.join(","),
    );
    check(
        ":root font-size",
        declaration(":root", "font-size"),
        px(typography.root_size),
    );
    check(
        ":root line-height",
        declaration(":root", "line-height"),
        typography.root_line_height.to_string(),
    );
    check(
        "body font-size",
        declaration("body", "font-size"),
        px(typography.body_size),
    );
    check(
        "h1 font-size",
        declaration("h1", "font-size"),
        px(typography.h1_size),
    );
    check(
        "h2 font-size",
        declaration("h2, h3", "font-size"),
        px(typography.h2_size),
    );
    check(
        "heading font-weight",
        declaration("h1, h2, h3", "font-weight"),
        typography.heading_weight.to_string(),
    );
    check(
        "heading letter-spacing",
        declaration("h1, h2, h3", "letter-spacing"),
        format!("{}em", typography.heading_letter_spacing),
    );

    // The steps and line heights styles.css leaves to Tailwind.
    for rule in &rules {
        for (name, _) in &rule.declarations {
            if name == "--text-xs" || name == "--text-xl" || name.ends_with("--line-height") {
                problems.push(format!(
                    "{} sets {name}, which the constants take from Tailwind's defaults",
                    rule.prelude
                ));
            }
        }
    }
    problems
}

#[test]
fn the_type_scale_matches_styles_css() {
    let problems = typography_problems(STYLES_CSS, &TYPOGRAPHY);

    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn a_type_scale_change_on_one_side_fails() {
    let changed = STYLES_CSS.replacen("--text-lg: 18px;", "--text-lg: 17px;", 1);
    let problems = typography_problems(&changed, &TYPOGRAPHY);
    assert!(
        problems.iter().any(|p| p.starts_with("--text-lg:")),
        "{problems:#?}"
    );

    let mut typography = TYPOGRAPHY;
    typography.body_size = 14.0;
    let problems = typography_problems(STYLES_CSS, &typography);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].starts_with("body font-size:"), "{problems:#?}");
}

/// `text-xs`, `text-xl`, every line height and the font weights are Tailwind's
/// defaults. Checked against `node_modules/tailwindcss/theme.css` when the frontend
/// dependencies are installed (`bun install`), skipped otherwise.
#[test]
fn tailwind_defaults_match_the_installed_tailwind() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../node_modules/tailwindcss/theme.css");
    let Ok(theme_css) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: node_modules/tailwindcss/theme.css is not installed");
        return;
    };
    let rules = css::rules(&theme_css);
    let defaults = find(&rules, "@theme default").expect("Tailwind's default theme");
    let get = |name: &str| defaults.get(name).unwrap_or_else(|| panic!("{name}"));
    let rem = |value: &str| -> f32 {
        value
            .strip_suffix("rem")
            .and_then(|number| number.parse::<f32>().ok())
            .map(|rem| rem * TYPOGRAPHY.root_size)
            .unwrap_or_else(|| panic!("not rem: {value}"))
    };
    // `calc(a / b)`
    let ratio = |value: &str| -> f32 {
        let inner = value
            .strip_prefix("calc(")
            .and_then(|rest| rest.strip_suffix(')'))
            .unwrap_or_else(|| panic!("not calc(): {value}"));
        let (a, b) = inner.split_once('/').expect("a division");
        a.trim().parse::<f32>().unwrap() / b.trim().parse::<f32>().unwrap()
    };

    assert_eq!(rem(get("--text-xs")), TYPOGRAPHY.xs.size);
    assert_eq!(rem(get("--text-xl")), TYPOGRAPHY.xl.size);
    for (name, step) in [
        ("--text-xs", TYPOGRAPHY.xs),
        ("--text-sm", TYPOGRAPHY.sm),
        ("--text-base", TYPOGRAPHY.base),
        ("--text-lg", TYPOGRAPHY.lg),
        ("--text-xl", TYPOGRAPHY.xl),
        ("--text-2xl", TYPOGRAPHY.xxl),
    ] {
        let line_height = ratio(get(&format!("{name}--line-height")));
        assert!(
            (line_height - step.line_height).abs() < 1e-6,
            "{name}: Tailwind {line_height}, Rust {}",
            step.line_height
        );
    }
    for (name, weight) in [
        ("--font-weight-medium", TYPOGRAPHY.weight_medium),
        ("--font-weight-semibold", TYPOGRAPHY.weight_semibold),
        ("--font-weight-bold", TYPOGRAPHY.weight_bold),
    ] {
        assert_eq!(get(name).parse::<f32>().ok(), Some(weight), "{name}");
    }
}
