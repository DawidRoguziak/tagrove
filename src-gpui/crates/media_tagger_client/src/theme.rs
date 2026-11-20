//! Theme tokens come from `src/styles.css`, the stylesheet of the Solid frontend, so
//! both interfaces draw the same colours. This first step reads the hex colour tokens
//! of the two DaisyUI theme blocks; the derived tokens and the full token set arrive
//! with epic E03.

const STYLES_CSS: &str = include_str!("../../../../src/styles.css");

const THEME_BLOCK: &str = "@plugin \"daisyui/theme\"";

/// An opaque sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// `0xRRGGBB`.
    pub fn to_u32(self) -> u32 {
        (u32::from(self.r) << 16) | (u32::from(self.g) << 8) | u32::from(self.b)
    }

    /// `#rgb` or `#rrggbb`.
    fn parse_hex(value: &str) -> Option<Self> {
        let digits = value.strip_prefix('#')?;
        if !digits.is_ascii() {
            return None;
        }
        let channel = |text: &str| u8::from_str_radix(text, 16).ok();
        match digits.len() {
            3 => {
                let wide = |index: usize| channel(&digits[index..=index]).map(|v| v * 17);
                Some(Self {
                    r: wide(0)?,
                    g: wide(1)?,
                    b: wide(2)?,
                })
            }
            6 => Some(Self {
                r: channel(&digits[0..2])?,
                g: channel(&digits[2..4])?,
                b: channel(&digits[4..6])?,
            }),
            _ => None,
        }
    }
}

/// The themes `src/styles.css` defines. Dark is the default, as in the Solid frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeName {
    Light,
    #[default]
    Dark,
}

impl ThemeName {
    /// The `name` of the theme block in the stylesheet.
    pub fn css_name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

/// Value of a declaration (`property: value;`) inside one block body.
fn declaration<'a>(body: &'a str, property: &str) -> Option<&'a str> {
    body.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        (name.trim() == property).then(|| value.trim())
    })
}

/// Bodies (the text between the braces) of every DaisyUI theme block.
fn theme_blocks(css: &str) -> impl Iterator<Item = &str> {
    css.split(THEME_BLOCK).skip(1).filter_map(|rest| {
        let open = rest.find('{')?;
        let close = open + rest[open..].find('}')?;
        // Only whitespace may separate the at-rule from its block.
        rest[..open]
            .trim()
            .is_empty()
            .then(|| &rest[open + 1..close])
    })
}

/// A hex colour token (for example `--color-base-200`) of one theme block of a
/// stylesheet. `None` when the theme or the token is missing or is not a hex colour.
pub fn css_color_token(css: &str, theme: ThemeName, token: &str) -> Option<Rgb> {
    let quoted_name = format!("\"{}\"", theme.css_name());
    let body = theme_blocks(css).find(|body| declaration(body, "name") == Some(&quoted_name))?;
    Rgb::parse_hex(declaration(body, token)?)
}

/// A hex colour token of the embedded `src/styles.css`.
pub fn color_token(theme: ThemeName, token: &str) -> Option<Rgb> {
    css_color_token(STYLES_CSS, theme, token)
}

#[cfg(test)]
mod tests {
    use super::{color_token, css_color_token, Rgb, ThemeName};

    const CSS: &str = r##"
:root { --color-base-200: #999999; }
@plugin "daisyui" { themes: false; }
@plugin "daisyui/theme" {
  name: "light";
  color-scheme: light;
  --color-base-200: #f4f6f5;
  --color-short: #0af;
  --noise: 0;
}
@plugin "daisyui/theme" {
  name: "dark";
  --color-base-200: #141618;
  --color-derived: color-mix(in srgb, #000 50%, transparent);
}
"##;

    #[test]
    fn reads_a_token_from_the_named_theme_block() {
        assert_eq!(
            css_color_token(CSS, ThemeName::Light, "--color-base-200"),
            Some(Rgb {
                r: 0xf4,
                g: 0xf6,
                b: 0xf5
            })
        );
        assert_eq!(
            css_color_token(CSS, ThemeName::Dark, "--color-base-200"),
            Some(Rgb {
                r: 0x14,
                g: 0x16,
                b: 0x18
            })
        );
    }

    #[test]
    fn expands_short_hex_and_rejects_everything_else() {
        assert_eq!(
            css_color_token(CSS, ThemeName::Light, "--color-short"),
            Some(Rgb {
                r: 0x00,
                g: 0xaa,
                b: 0xff
            })
        );
        assert_eq!(css_color_token(CSS, ThemeName::Light, "--noise"), None);
        assert_eq!(
            css_color_token(CSS, ThemeName::Dark, "--color-derived"),
            None
        );
        assert_eq!(css_color_token(CSS, ThemeName::Dark, "--color-short"), None);
        assert_eq!(
            css_color_token(CSS, ThemeName::Dark, "--color-base"),
            None,
            "a token name is matched whole"
        );
        assert_eq!(
            css_color_token("", ThemeName::Dark, "--color-base-200"),
            None
        );
    }

    #[test]
    fn packs_channels_as_rrggbb() {
        let color = Rgb {
            r: 0x14,
            g: 0x16,
            b: 0x18,
        };
        assert_eq!(color.to_u32(), 0x141618);
    }

    #[test]
    fn embedded_stylesheet_defines_the_window_tokens_of_both_themes() {
        for theme in [ThemeName::Light, ThemeName::Dark] {
            for token in [
                "--color-base-100",
                "--color-base-200",
                "--color-base-content",
            ] {
                assert!(
                    color_token(theme, token).is_some(),
                    "{token} of the {} theme",
                    theme.css_name()
                );
            }
        }
        assert_eq!(ThemeName::default(), ThemeName::Dark);
    }
}
