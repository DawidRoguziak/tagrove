use tauri::{window::Color, Theme, WebviewWindow};

#[tauri::command]
pub fn sync_window_theme(theme: String, window: WebviewWindow) -> Result<(), String> {
    let (window_theme, background_color) = resolve_window_theme(&theme)?;

    window
        .set_theme(Some(window_theme))
        .map_err(|error| error.to_string())?;
    window
        .set_background_color(Some(background_color))
        .map_err(|error| error.to_string())?;

    Ok(())
}

fn resolve_window_theme(theme: &str) -> Result<(Theme, Color), String> {
    match theme {
        "light" => Ok((Theme::Light, Color(0xED, 0xE8, 0xDD, 0xFF))),
        "dark" => Ok((Theme::Dark, Color(0x12, 0x12, 0x12, 0xFF))),
        _ => Err(format!("unsupported theme '{theme}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_light_theme() {
        let (theme, color) = resolve_window_theme("light").unwrap();
        assert_eq!(theme, Theme::Light);
        assert_eq!(color, Color(0xED, 0xE8, 0xDD, 0xFF));
    }

    #[test]
    fn resolves_dark_theme() {
        let (theme, color) = resolve_window_theme("dark").unwrap();
        assert_eq!(theme, Theme::Dark);
        assert_eq!(color, Color(0x12, 0x12, 0x12, 0xFF));
    }

    #[test]
    fn rejects_unknown_empty_and_differently_cased_themes() {
        for input in [
            "", "system", "auto", "Light", "DARK", " dark", "dark ", "light\n",
        ] {
            let error = resolve_window_theme(input).unwrap_err();
            assert_eq!(error, format!("unsupported theme '{input}'"));
        }
    }
}
