//! Fit, zoom and pan of the image viewer (`lightboxPanZoomService.ts`): pure
//! calculations on the size of the picture and of the stage it is shown in.
//!
//! Lengths are CSS pixels. A picture is first fitted into the stage (it is scaled up
//! as well as down), and the zoom factor multiplies that fitted size: 1 shows the
//! whole picture, [`MAX_ZOOM_FACTOR`] twelve times as large. The pan is the offset of the
//! picture's centre from the centre of the stage.

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MediaDimensions {
    pub width: f64,
    pub height: f64,
}

impl MediaDimensions {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }

    /// Neither side is zero or not a number (`!size.width || !size.height`).
    fn is_known(&self) -> bool {
        truthy(self.width) && truthy(self.height)
    }
}

/// JavaScript's truthiness of a number.
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

/// A picture fitted into a stage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FittedMedia {
    /// Displayed pixels per picture pixel at zoom 1.
    pub fit_scale: f64,
    pub width: f64,
    pub height: f64,
}

impl FittedMedia {
    pub fn dimensions(&self) -> MediaDimensions {
        MediaDimensions::new(self.width, self.height)
    }
}

/// An offset of the picture's centre from the centre of the stage.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pan {
    pub x: f64,
    pub y: f64,
}

impl Pan {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

pub const MIN_ZOOM_FACTOR: f64 = 1.0;
pub const MAX_ZOOM_FACTOR: f64 = 12.0;
/// One step of the `+` and `-` keys and of a click.
pub const ZOOM_MULTIPLIER: f64 = 1.25;

const DOUBLE_CLICK_BASE_ZOOM_FACTOR: f64 = 2.0;
const ZOOM_EPSILON: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomDirection {
    In,
    Out,
}

/// `calculateFitScale`: the scale at which the whole picture fits the stage; 1 while
/// either size is unknown.
pub fn calculate_fit_scale(intrinsic: MediaDimensions, media: MediaDimensions) -> f64 {
    if !intrinsic.is_known() || !media.is_known() {
        return 1.0;
    }
    (media.width / intrinsic.width).min(media.height / intrinsic.height)
}

/// `calculateFittedMediaSize`: no size while either size is unknown.
pub fn calculate_fitted_media_size(
    intrinsic: MediaDimensions,
    media: MediaDimensions,
) -> FittedMedia {
    if !intrinsic.is_known() || !media.is_known() {
        return FittedMedia {
            fit_scale: 1.0,
            width: 0.0,
            height: 0.0,
        };
    }
    let fit_scale = calculate_fit_scale(intrinsic, media);
    FittedMedia {
        fit_scale,
        width: intrinsic.width * fit_scale,
        height: intrinsic.height * fit_scale,
    }
}

/// `clampZoomFactor`.
pub fn clamp_zoom_factor(next: f64) -> f64 {
    // `Math.max(MIN, Math.min(MAX, next))`: not a number stays not a number there;
    // here it becomes the minimum, which no caller can produce anyway.
    if next.is_nan() {
        return MIN_ZOOM_FACTOR;
    }
    next.clamp(MIN_ZOOM_FACTOR, MAX_ZOOM_FACTOR)
}

/// `getNextZoomFactor`: one step in or out, within the limits.
pub fn next_zoom_factor(current: f64, direction: ZoomDirection) -> f64 {
    clamp_zoom_factor(match direction {
        ZoomDirection::In => current * ZOOM_MULTIPLIER,
        ZoomDirection::Out => current / ZOOM_MULTIPLIER,
    })
}

/// `getDoubleClickZoomFactor`: the factor at which one picture pixel is one
/// displayed pixel, at least 2. The Solid modal binds no double click to it (a double
/// click there is two single clicks), and neither does the GPUI view; see
/// `docs/subsystems/lightbox.md`.
pub fn double_click_zoom_factor(fit_scale: f64) -> f64 {
    let pixel_perfect = if fit_scale > 0.0 {
        1.0 / fit_scale
    } else {
        DOUBLE_CLICK_BASE_ZOOM_FACTOR
    };
    clamp_zoom_factor(DOUBLE_CLICK_BASE_ZOOM_FACTOR.max(pixel_perfect))
}

/// `clampPan`: the picture never leaves an edge of the stage it overflows, and is
/// centred along an axis it does not overflow. No pan at zoom 1.
pub fn clamp_pan(next: Pan, zoom: f64, fitted: MediaDimensions, media: MediaDimensions) -> Pan {
    if zoom <= MIN_ZOOM_FACTOR + ZOOM_EPSILON {
        return Pan::ZERO;
    }
    let max_x = (fitted.width * zoom - media.width).max(0.0) / 2.0;
    let max_y = (fitted.height * zoom - media.height).max(0.0) / 2.0;
    Pan {
        x: next.x.min(max_x).max(-max_x),
        y: next.y.min(max_y).max(-max_y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAGE: MediaDimensions = MediaDimensions::new(800.0, 600.0);

    #[test]
    fn a_picture_is_fitted_into_the_stage_in_both_directions() {
        let large = calculate_fitted_media_size(MediaDimensions::new(1600.0, 600.0), STAGE);
        assert_eq!(
            (large.fit_scale, large.width, large.height),
            (0.5, 800.0, 300.0)
        );
        // A small picture is scaled up until one side touches.
        let small = calculate_fitted_media_size(MediaDimensions::new(40.0, 40.0), STAGE);
        assert_eq!(
            (small.fit_scale, small.width, small.height),
            (15.0, 600.0, 600.0)
        );
    }

    #[test]
    fn an_unknown_size_fits_nothing() {
        for (intrinsic, media) in [
            (MediaDimensions::default(), STAGE),
            (MediaDimensions::new(100.0, 0.0), STAGE),
            (
                MediaDimensions::new(100.0, 100.0),
                MediaDimensions::default(),
            ),
            (MediaDimensions::new(f64::NAN, 100.0), STAGE),
        ] {
            assert_eq!(calculate_fit_scale(intrinsic, media), 1.0);
            let fitted = calculate_fitted_media_size(intrinsic, media);
            assert_eq!(
                (fitted.fit_scale, fitted.width, fitted.height),
                (1.0, 0.0, 0.0)
            );
        }
    }

    #[test]
    fn zoom_steps_stay_between_one_and_twelve() {
        assert_eq!(clamp_zoom_factor(0.2), 1.0);
        assert_eq!(clamp_zoom_factor(40.0), 12.0);
        assert_eq!(clamp_zoom_factor(3.5), 3.5);
        assert_eq!(next_zoom_factor(1.0, ZoomDirection::In), 1.25);
        assert_eq!(next_zoom_factor(1.25, ZoomDirection::Out), 1.0);
        assert_eq!(next_zoom_factor(1.0, ZoomDirection::Out), 1.0);
        assert_eq!(next_zoom_factor(11.0, ZoomDirection::In), 12.0);
    }

    #[test]
    fn the_double_click_factor_is_pixel_perfect_and_at_least_two() {
        assert_eq!(double_click_zoom_factor(0.25), 4.0);
        assert_eq!(double_click_zoom_factor(0.8), 2.0);
        assert_eq!(double_click_zoom_factor(0.0), 2.0);
        assert_eq!(double_click_zoom_factor(0.01), 12.0);
    }

    #[test]
    fn the_pan_is_bounded_by_what_overflows_the_stage() {
        let fitted = MediaDimensions::new(800.0, 300.0);
        // At zoom 2 the picture is 1600 x 600: 800 overflow across, none down.
        assert_eq!(
            clamp_pan(Pan::new(900.0, 50.0), 2.0, fitted, STAGE),
            Pan::new(400.0, 0.0)
        );
        assert_eq!(
            clamp_pan(Pan::new(-900.0, -50.0), 2.0, fitted, STAGE),
            Pan::new(-400.0, 0.0)
        );
        assert_eq!(
            clamp_pan(Pan::new(120.0, 0.0), 2.0, fitted, STAGE),
            Pan::new(120.0, 0.0)
        );
        // No pan at zoom 1, whatever was asked for.
        assert_eq!(
            clamp_pan(Pan::new(50.0, 50.0), 1.0, fitted, STAGE),
            Pan::ZERO
        );
        assert_eq!(
            clamp_pan(Pan::new(50.0, 50.0), 1.0005, fitted, STAGE),
            Pan::ZERO
        );
    }
}
