//! The Tagrove logo (`public/tagrove.svg`, the image the Tauri header shows), drawn in
//! its own colours.

use std::sync::{Arc, LazyLock};

use gpui::{img, px, Image, ImageFormat, Img, Styled as _};

/// The shared logo file, built into the binary.
pub const LOGO_SVG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../public/tagrove.svg"
));

static LOGO: LazyLock<Arc<Image>> =
    LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Svg, LOGO_SVG.to_vec())));

/// The logo, `size` CSS pixels square.
pub fn logo(size: f32) -> Img {
    img(LOGO.clone()).size(px(size)).flex_none()
}
