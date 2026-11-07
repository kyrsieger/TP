//! Native image presentation shared by desktop hosts, independent of their UI toolkit.
use super::{
    common::{err, Result},
    media,
    storage::Store,
};
use std::time::Instant;

#[cfg(any(target_os = "macos", test))]
mod mac_drag;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

pub struct Picture {
    pub id: String,
    pub index: usize,
    pub count: usize,
    pub video: bool,
    pub unavailable: Option<&'static str>,
    pub image: image::RgbaImage,
    #[cfg(target_os = "macos")]
    pub encoded: Vec<u8>,
}

fn decode_picture(data: String) -> Result<image::RgbaImage> {
    use base64::Engine;
    use image::ImageDecoder;
    let (_, data) = data
        .split_once(',')
        .ok_or(crate::i18n::tr("无效的预览图片"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(err)?;
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(err)?
        .into_decoder()
        .map_err(err)?;
    let orientation = decoder.orientation().map_err(err)?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(err)?;
    decoded.apply_orientation(orientation);
    Ok(decoded.to_rgba8())
}

fn picture_from_data(
    id: String,
    index: usize,
    count: usize,
    video: bool,
    data: Result<String>,
) -> Result<Picture> {
    let (image, unavailable) = match data.and_then(decode_picture) {
        Ok(image) => (image, None),
        // A missing or undecodable item must not prevent visiting the rest of a group.
        Err(_) if count > 1 => (
            image::RgbaImage::from_pixel(640, 360, image::Rgba([24, 24, 24, 255])),
            Some(if video {
                "首帧预览不可用"
            } else {
                "预览图不可用"
            }),
        ),
        Err(error) => return Err(error),
    };
    #[cfg(target_os = "macos")]
    let encoded = {
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(err)?;
        output.into_inner()
    };
    Ok(Picture {
        id,
        index,
        count,
        video,
        unavailable,
        image,
        #[cfg(target_os = "macos")]
        encoded,
    })
}

/// Host integration for the native preview. All callbacks run on the UI thread.
#[derive(Clone)]
pub struct Callbacks {
    /// Windows owner HWND; ignored on macOS. Zero uses the preview's monitor.
    pub owner: usize,
    pub navigate: std::sync::Arc<dyn Fn(String, usize) + Send + Sync>,
    pub closed: std::sync::Arc<dyn Fn() + Send + Sync>,
}

/// Load on a worker; present with `show` on the host's UI thread.
pub fn load(store: &Store, id: String, index: Option<usize>) -> Result<Picture> {
    let (kind, files): (String, String) = store
        .db
        .lock()
        .map_err(err)?
        .query_row("SELECT kind,files FROM records WHERE id=?1", [&id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(err)?;
    let count = serde_json::from_str::<Vec<serde_json::Value>>(&files)
        .map_err(err)?
        .len()
        .max(1);
    let index = index.unwrap_or(0).min(count - 1);
    let data = media::preview_data(
        store,
        id.clone(),
        Some(if kind == "video" { "preview" } else { "full" }.into()),
        Some(index),
    );
    picture_from_data(id, index, count, kind == "video", data)
}

/// Must be called on the application's UI thread.
pub fn show(callbacks: Callbacks, picture: Picture) -> Result<()> {
    platform::show(callbacks, picture)
}
/// Must be called on the application's UI thread.
pub fn hide() {
    platform::hide();
}
/// Must be called on the application's UI thread.
pub fn refresh_language() {
    platform::refresh_language();
}
fn closed(callbacks: &Callbacks) {
    (callbacks.closed)();
}
fn step(callbacks: &Callbacks, id: &str, index: usize, count: usize, direction: isize) {
    if count > 1 {
        (callbacks.navigate)(
            id.to_owned(),
            (index as isize + direction).rem_euclid(count as isize) as usize,
        );
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            w: (self.x + self.w).min(other.x + other.w) - x,
            h: (self.y + self.h).min(other.y + other.h) - y,
        }
    }
}
pub(super) struct Geometry {
    pub width: f64,
    pub height: f64,
    pub dpi: f64,
    pub scale: f64,
    pub target: f64,
    pub x: f64,
    pub y: f64,
    pub work: Rect,
    anchor: (f64, f64),
    image_anchor: (f64, f64),
    last: Instant,
}
impl Geometry {
    pub fn new(width: u32, height: u32, dpi: f64, work: Rect) -> Self {
        let mut s = Self {
            width: width as f64,
            height: height as f64,
            dpi,
            scale: 1.,
            target: 1.,
            x: 0.,
            y: 0.,
            work,
            anchor: (0., 0.),
            image_anchor: (0., 0.),
            last: Instant::now(),
        };
        s.fit();
        s
    }
    pub fn fit_scale(&self) -> f64 {
        1f64.min((self.work.w * 0.9).min(1600. * self.dpi) / (self.width * self.dpi))
            .min((self.work.h * 0.9).min(1000. * self.dpi) / (self.height * self.dpi))
    }
    pub fn fit(&mut self) {
        self.scale = self.fit_scale();
        self.center();
    }
    pub fn fill_scale(&self) -> f64 {
        (self.work.w / (self.width * self.dpi)).min(self.work.h / (self.height * self.dpi))
    }
    pub fn fill(&mut self) {
        self.scale = self.fill_scale();
        self.center();
    }
    fn center(&mut self) {
        self.target = self.scale;
        self.x = self.work.x + (self.work.w - self.width * self.dpi * self.scale) / 2.;
        self.y = self.work.y + (self.work.h - self.height * self.dpi * self.scale) / 2.;
    }
    pub fn rect(&self) -> Rect {
        Rect {
            x: self.x.round(),
            y: self.y.round(),
            w: (self.width * self.dpi * self.scale).round().max(1.),
            h: (self.height * self.dpi * self.scale).round().max(1.),
        }
    }
    pub fn zoom(&mut self, factor: f64, anchor: (f64, f64)) {
        if !factor.is_finite() || factor <= 0. {
            return;
        }
        self.image_anchor = (
            (anchor.0 - self.x) / self.scale,
            (anchor.1 - self.y) / self.scale,
        );
        self.anchor = anchor;
        self.target =
            (self.target * factor).clamp(0.1f64.min(self.fit_scale()), 8f64.max(self.fill_scale()));
        // Do not restart the clock for every event: high-rate input must not starve animation.
        if (self.scale - self.target).abs() < 0.000001 {
            self.last = Instant::now();
        }
    }
    pub fn tick(&mut self) -> bool {
        let dt = self.last.elapsed().as_secs_f64().min(0.05);
        self.last = Instant::now();
        if self.scale == self.target {
            return false;
        }
        self.scale += (self.target - self.scale) * (1. - (-dt / 0.035).exp());
        if (self.scale - self.target).abs() < 0.0005 {
            self.scale = self.target;
        }
        self.x = self.anchor.0 - self.image_anchor.0 * self.scale;
        self.y = self.anchor.1 - self.image_anchor.1 * self.scale;
        true
    }
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.x = x;
        self.y = y;
        self.target = self.scale;
    }
    #[cfg(windows)]
    pub fn update_dpi(&mut self, dpi: f64, anchor: (f64, f64)) -> bool {
        if !dpi.is_finite() || dpi <= 0. || self.dpi == dpi {
            return false;
        }
        // Preserve the image point under the pointer and the user's zoom percentage.
        // Rebase the animation too, so its next tick cannot restore the old geometry.
        let ratio = dpi / self.dpi;
        self.x = anchor.0 - (anchor.0 - self.x) * ratio;
        self.y = anchor.1 - (anchor.1 - self.y) * ratio;
        self.dpi = dpi;
        self.anchor = anchor;
        self.image_anchor = (
            (anchor.0 - self.x) / self.scale,
            (anchor.1 - self.y) / self.scale,
        );
        true
    }
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Close,
    Smaller,
    Bigger,
    Fit,
    Fill,
    Original,
    Previous,
    Next,
}
pub(super) struct Control {
    pub rect: Rect,
    pub text: String,
    pub action: Option<Action>,
}
pub(super) fn controls(
    width: f64,
    height: f64,
    dpi: f64,
    scale: f64,
    index: usize,
    count: usize,
) -> Vec<Control> {
    let compact = width < 100. * dpi || height < 50. * dpi;
    let size = ((if compact { 16. } else { 30. }) * dpi)
        .min(height)
        .min(width);
    let gap = (if compact { 1. } else { 4. }) * dpi;
    let margin = (if compact { 1. } else { 8. }) * dpi;
    let mut x = (width - margin - size).max(0.);
    let y = margin.min((height - size).max(0.));
    let mut items = vec![Control {
        rect: Rect {
            x,
            y,
            w: size,
            h: size,
        },
        text: "×".into(),
        action: Some(Action::Close),
    }];
    let percent_width = (46. * dpi).min((x - gap).max(0.));
    let show_plus = !compact && x >= size + gap + percent_width + gap;
    let mut add = |text: String, action: Option<Action>, w: f64| {
        if x >= w + gap {
            x -= w + gap;
            items.push(Control {
                rect: Rect { x, y, w, h: size },
                text,
                action,
            });
        }
    };
    if show_plus {
        add("+".into(), Some(Action::Bigger), size);
    }
    if percent_width >= 20. * dpi {
        add(format!("{}%", (scale * 100.).round()), None, percent_width);
    }
    if compact {
        return items;
    }
    add("−".into(), Some(Action::Smaller), size);
    add(
        super::i18n::choose("合适", "Fit").into(),
        Some(Action::Fit),
        40. * dpi,
    );
    add(
        super::i18n::choose("撑满", "Fill").into(),
        Some(Action::Fill),
        40. * dpi,
    );
    add(
        super::i18n::choose("原图", "Original").into(),
        Some(Action::Original),
        if super::i18n::chinese() {
            40. * dpi
        } else {
            64. * dpi
        },
    );
    if count > 1 {
        add("›".into(), Some(Action::Next), size);
        add(format!("{}/{}", index + 1, count), None, 48. * dpi);
        add("‹".into(), Some(Action::Previous), size);
    }
    items
}

#[cfg(test)]
#[path = "tests/failed_items.rs"]
mod failed_items_tests;

#[cfg(test)]
mod language_tests {
    use super::*;
    #[test]
    fn english_controls_have_room_and_stay_inside_the_preview() {
        crate::i18n::with_test_language(false, || {
            let items = controls(800., 600., 1., 1., 0, 2);
            let original = items.iter().find(|item| item.text == "Original").unwrap();
            assert!(original.rect.w >= 64.);
            assert!(items.iter().any(|item| item.text == "Fit"));
            assert!(items
                .iter()
                .all(|item| item.rect.x >= 0. && item.rect.x + item.rect.w <= 800.));
        });
    }
}
