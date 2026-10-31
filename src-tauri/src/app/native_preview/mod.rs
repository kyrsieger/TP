//! Native image presentation. The WebView is deliberately outside the resize path.
use super::{
    common::{err, Result},
    media,
    storage::Store,
};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
use tauri::{Emitter, Manager};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", test))]
mod mac_drag;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

#[derive(Default)]
pub(crate) struct Requests(AtomicU64);
pub(super) struct Picture {
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
    let (_, data) = data.split_once(',').ok_or("无效的预览图片")?;
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

#[tauri::command]
pub(crate) async fn open_native_large_preview(
    app: tauri::AppHandle,
    id: String,
    index: Option<usize>,
) -> Result<bool> {
    let generation = app.state::<Requests>().0.fetch_add(1, Ordering::SeqCst) + 1;
    load_picture(app, id, index, generation).await
}

async fn load_picture(
    app: tauri::AppHandle,
    id: String,
    index: Option<usize>,
    generation: u64,
) -> Result<bool> {
    let loader = app.clone();
    let picture = tauri::async_runtime::spawn_blocking(move || -> Result<Picture> {
        let store = loader.state::<Store>();
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
    })
    .await
    .map_err(err)??;
    if app.state::<Requests>().0.load(Ordering::SeqCst) != generation {
        return Ok(true);
    }
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    let main = app.clone();
    app.run_on_main_thread(move || {
        let result = if main.state::<Requests>().0.load(Ordering::SeqCst) == generation {
            platform::show(main, picture)
        } else {
            Ok(())
        };
        let _ = tx.try_send(result);
    })
    .map_err(err)?;
    rx.recv().await.ok_or("预览加载已取消")??;
    Ok(true)
}

pub(crate) fn hide(app: &tauri::AppHandle) -> Result<()> {
    app.state::<Requests>().0.fetch_add(1, Ordering::SeqCst);
    app.run_on_main_thread(platform::hide).map_err(err)
}
pub(super) fn closed(app: &tauri::AppHandle) {
    app.state::<Requests>().0.fetch_add(1, Ordering::SeqCst);
    let _ = app.emit_to("main", "large-preview-closed", ());
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.set_focus();
    }
}
pub(super) fn step(app: &tauri::AppHandle, id: &str, index: usize, count: usize, direction: isize) {
    if count < 2 {
        return;
    }
    // Reserve before spawning so closing immediately after a key press cancels it too.
    let generation = app.state::<Requests>().0.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    let id = id.to_owned();
    let next = (index as isize + direction).rem_euclid(count as isize) as usize;
    tauri::async_runtime::spawn(async move {
        if let Err(error) = load_picture(app.clone(), id, Some(next), generation).await {
            if app.state::<Requests>().0.load(Ordering::SeqCst) == generation {
                let _ = app.emit_to("main", "native-preview-error", error);
            }
        }
    });
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
    add("合适".into(), Some(Action::Fit), 40. * dpi);
    add("撑满".into(), Some(Action::Fill), 40. * dpi);
    add("原图".into(), Some(Action::Original), 40. * dpi);
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
