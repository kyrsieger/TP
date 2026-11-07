//! Original lucide-react 1.57.0 paths; see assets/icons/LICENSE.
use gpui::{px, AssetSource, SharedString, Styled};
use std::borrow::Cow;
pub struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let data: &'static [u8] = match path {
            "icons/language-zh.svg" => include_bytes!("../assets/icons/language-zh.svg"),
            "icons/language-en.svg" => include_bytes!("../assets/icons/language-en.svg"),
            "icons/tp-mark.svg" => include_bytes!("../../../packaging/icons/tp-mark.svg"),
            "icons/pin.svg" => include_bytes!("../assets/icons/pin.svg"),
            "icons/star.svg" => include_bytes!("../assets/icons/star.svg"),
            "icons/star-fill.svg" => include_bytes!("../assets/icons/star-fill.svg"),
            "icons/group.svg" => include_bytes!("../assets/icons/group.svg"),
            "icons/x.svg" => include_bytes!("../assets/icons/x.svg"),
            "icons/search.svg" => include_bytes!("../assets/icons/search.svg"),
            "icons/gear.svg" => include_bytes!("../assets/icons/gear.svg"),
            "icons/filter.svg" => include_bytes!("../assets/icons/filter.svg"),
            "icons/plus.svg" => include_bytes!("../assets/icons/plus.svg"),
            "icons/minus.svg" => include_bytes!("../assets/icons/minus.svg"),
            "icons/square.svg" => include_bytes!("../assets/icons/square.svg"),
            "icons/link.svg" => include_bytes!("../assets/icons/link.svg"),
            "icons/file.svg" => include_bytes!("../assets/icons/file.svg"),
            "icons/image.svg" => include_bytes!("../assets/icons/image.svg"),
            "icons/video.svg" => include_bytes!("../assets/icons/video.svg"),
            "icons/text.svg" => include_bytes!("../assets/icons/text.svg"),
            "icons/color.svg" => include_bytes!("../assets/icons/color.svg"),
            "icons/play-fill.svg" => include_bytes!("../assets/icons/play-fill.svg"),
            "icons/play.svg" => include_bytes!("../assets/icons/play.svg"),
            "icons/check.svg" => include_bytes!("../assets/icons/check.svg"),
            "icons/chevron.svg" => include_bytes!("../assets/icons/chevron.svg"),
            "icons/clipboard.svg" => include_bytes!("../assets/icons/clipboard.svg"),
            "icons/arrow-up-to-line.svg" => include_bytes!("../assets/icons/arrow-up-to-line.svg"),
            "icons/circle.svg" => include_bytes!("../assets/icons/circle.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(data)))
    }
    fn list(&self, _: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
pub fn icon(name: &str, size: f32) -> gpui::Svg {
    gpui::svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_shrink_0()
}
