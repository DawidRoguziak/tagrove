//! What the placeholder window of epic E00 shows: one text of the shared locale
//! files on the application background of the shared stylesheet. The GPUI view only
//! draws these values.

use std::fmt::{Display, Formatter};

use crate::{
    i18n::Catalog,
    theme::{color_token, Rgba, ThemeName},
};

/// Locale key of the placeholder text.
pub const HELLO_TEXT_KEY: &str = "common.loading";
/// The application background (`--bg-0` in `src/styles.css`).
pub const BACKGROUND_TOKEN: &str = "--color-base-200";
/// The default text colour.
pub const FOREGROUND_TOKEN: &str = "--color-base-content";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelloScreen {
    pub text: String,
    pub background: Rgba,
    pub foreground: Rgba,
}

/// A shared source file misses what the placeholder window needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelloError {
    Catalog(String),
    MissingText(&'static str),
    MissingToken(&'static str),
}

impl Display for HelloError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(error) => write!(f, "cannot read the English locale file: {error}"),
            Self::MissingText(key) => write!(f, "locale key {key} is missing"),
            Self::MissingToken(token) => write!(f, "theme token {token} is missing"),
        }
    }
}

impl std::error::Error for HelloError {}

impl HelloScreen {
    /// English text and the colours of `theme`, from the embedded shared sources.
    pub fn load(theme: ThemeName) -> Result<Self, HelloError> {
        let catalog = Catalog::english().map_err(|error| HelloError::Catalog(error.to_string()))?;
        let token = |name| color_token(theme, name).ok_or(HelloError::MissingToken(name));
        Ok(Self {
            text: catalog
                .text(HELLO_TEXT_KEY)
                .ok_or(HelloError::MissingText(HELLO_TEXT_KEY))?
                .to_owned(),
            background: token(BACKGROUND_TOKEN)?,
            foreground: token(FOREGROUND_TOKEN)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::HelloScreen;
    use crate::theme::{Rgba, ThemeName};

    #[test]
    fn dark_screen_uses_the_shared_text_and_tokens() {
        let screen = HelloScreen::load(ThemeName::Dark).expect("shared sources are complete");

        assert_eq!(screen.text, "Loading...");
        assert_eq!(screen.background, Rgba::hex(0x141618));
        assert_eq!(screen.foreground, Rgba::hex(0xf4f6f5));
    }

    #[test]
    fn light_screen_differs_only_in_colours() {
        let dark = HelloScreen::load(ThemeName::Dark).unwrap();
        let light = HelloScreen::load(ThemeName::Light).unwrap();

        assert_eq!(light.text, dark.text);
        assert_ne!(light.background, dark.background);
        assert_ne!(light.foreground, dark.foreground);
    }
}
