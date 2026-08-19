use tauri::{window::Color, Theme, WebviewWindow};

#[tauri::command]
pub fn sync_window_theme(theme: String, window: WebviewWindow) -> Result<(), String> {
    let (window_theme, background_color, text_color) = resolve_window_theme(&theme)?;

    window
        .set_theme(Some(window_theme))
        .map_err(|error| error.to_string())?;
    window
        .set_background_color(Some(background_color))
        .map_err(|error| error.to_string())?;

    #[cfg(windows)]
    apply_windows_chrome_colors(&window, background_color, text_color)?;

    #[cfg(not(windows))]
    let _ = text_color;

    Ok(())
}

fn resolve_window_theme(theme: &str) -> Result<(Theme, Color, Color), String> {
    match theme {
        "light" => Ok((
            Theme::Light,
            Color(0xED, 0xE8, 0xDD, 0xFF),
            Color(0x1A, 0x1A, 0x1A, 0xFF),
        )),
        "dark" => Ok((
            Theme::Dark,
            Color(0x12, 0x12, 0x12, 0xFF),
            Color(0xF0, 0xF0, 0xE8, 0xFF),
        )),
        _ => Err(format!("unsupported theme '{theme}'")),
    }
}

#[cfg(windows)]
fn apply_windows_chrome_colors(
    window: &WebviewWindow,
    background_color: Color,
    text_color: Color,
) -> Result<(), String> {
    use std::mem::size_of;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let caption_color = to_colorref(background_color);
    let border_color = caption_color;
    let title_text_color = to_colorref(text_color);

    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption_color as *const _ as _,
            size_of::<u32>() as u32,
        )
        .map_err(|error| error.to_string())?;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &border_color as *const _ as _,
            size_of::<u32>() as u32,
        )
        .map_err(|error| error.to_string())?;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &title_text_color as *const _ as _,
            size_of::<u32>() as u32,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[cfg(windows)]
fn to_colorref(color: Color) -> u32 {
    u32::from(color.0) | (u32::from(color.1) << 8) | (u32::from(color.2) << 16)
}
