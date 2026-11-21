//! The two fonts of `src/styles.css` (`@font-face` Inter and JetBrains Mono), bundled
//! from `public/fonts` like the Tauri build bundles them, so the GPUI interface draws
//! the same faces without relying on the system's fonts.

use std::borrow::Cow;

use gpui::App;
use media_tagger_client::theme::TYPOGRAPHY;

/// The family of ordinary text (`font-family` of `:root`).
pub const SANS: &str = TYPOGRAPHY.sans[0];
/// `--font-mono`: chips, counters, paths.
pub const MONO: &str = TYPOGRAPHY.mono[0];

const INTER: &[u8] = include_bytes!("../../../../../public/fonts/Inter.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../../../../public/fonts/JetBrainsMono.ttf");

/// Registers both fonts with the text system. Call once at startup.
pub fn register(cx: &App) -> gpui::Result<()> {
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(INTER), Cow::Borrowed(JETBRAINS_MONO)])
}

#[cfg(test)]
mod tests {
    use super::{INTER, JETBRAINS_MONO, MONO, SANS};

    #[test]
    fn the_families_are_the_first_of_the_stylesheet() {
        let stylesheet = include_str!("../../../../../src/styles.css");
        assert!(stylesheet.contains(r#"font-family: "Inter";"#));
        assert!(stylesheet.contains(r#"src: url("/fonts/Inter.ttf")"#));
        assert!(stylesheet.contains(r#"font-family: "JetBrains Mono";"#));
        assert!(stylesheet.contains(r#"src: url("/fonts/JetBrainsMono.ttf")"#));
        assert_eq!((SANS, MONO), ("Inter", "JetBrains Mono"));
        // TrueType files (0x00010000 version tag).
        for font in [INTER, JETBRAINS_MONO] {
            assert_eq!(&font[..4], &[0, 1, 0, 0]);
        }
    }
}
