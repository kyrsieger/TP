//! Shared language selection for native UI and command messages.
use super::{
    common::{err, Result},
    storage::{settings, Store},
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use tauri::Emitter;

static PREFERENCE: AtomicU8 = AtomicU8::new(0);
// Keep startup/migration diagnostics usable before settings are loaded.
static CHINESE: AtomicBool = AtomicBool::new(true);
static SYSTEM_CHINESE: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Serialize)]
pub(super) struct LanguageState {
    preference: &'static str,
    resolved: &'static str,
    system: &'static str,
}

fn resolve(preference: &str, system: &str) -> &'static str {
    match preference {
        "zh-CN" => "zh-CN",
        "en" => "en",
        _ if system
            .split(['-', '_'])
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case("zh")) =>
        {
            "zh-CN"
        }
        _ => "en",
    }
}

fn system_language() -> String {
    #[cfg(windows)]
    {
        // The low 10 bits identify the primary language, independently of region.
        let language = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        if language & 0x03ff == 0x04 {
            "zh-CN".into()
        } else {
            "en".into()
        }
    }
    #[cfg(target_os = "macos")]
    {
        objc2_foundation::NSLocale::preferredLanguages()
            .firstObject()
            .map(|language| language.to_string())
            .unwrap_or_else(|| "en".into())
    }
}

fn state() -> LanguageState {
    let preference = match PREFERENCE.load(Ordering::Relaxed) {
        1 => "zh-CN",
        2 => "en",
        _ => "system",
    };
    let system = resolve("system", &system_language());
    LanguageState {
        preference,
        resolved: resolve(preference, system),
        system,
    }
}
pub(super) fn set_preference(preference: &str) {
    PREFERENCE.store(
        match preference {
            "zh-CN" => 1,
            "en" => 2,
            _ => 0,
        },
        Ordering::Relaxed,
    );
    let language = state();
    CHINESE.store(language.resolved == "zh-CN", Ordering::Relaxed);
    SYSTEM_CHINESE.store(language.system == "zh-CN", Ordering::Relaxed);
}
#[cfg(test)]
thread_local! { static TEST_LANGUAGE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }

pub(crate) fn chinese() -> bool {
    #[cfg(test)]
    if let Some(chinese) = TEST_LANGUAGE.with(|language| language.get()) {
        return chinese;
    }
    CHINESE.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn with_test_language<T>(chinese: bool, action: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_LANGUAGE.with(|language| language.set(self.0));
        }
    }
    let _reset = Reset(TEST_LANGUAGE.with(|language| language.replace(Some(chinese))));
    action()
}
pub(super) fn choose<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if chinese() {
        zh
    } else {
        en
    }
}

#[tauri::command]
pub(super) fn get_language(store: tauri::State<Store>) -> Result<LanguageState> {
    let conn = store.db.lock().map_err(err)?;
    let preference = settings(&conn)?.language;
    let system = resolve("system", &system_language());
    let resolved = resolve(&preference, system);
    Ok(LanguageState {
        preference: match preference.as_str() {
            "zh-CN" => "zh-CN",
            "en" => "en",
            _ => "system",
        },
        resolved,
        system,
    })
}

pub(super) fn changed(app: &tauri::AppHandle) {
    if let Err(error) = super::runtime::update_tray_language(app) {
        eprintln!("Failed to update tray language: {error}");
    }
    if let Err(error) = super::native_preview::refresh_language(app) {
        eprintln!("Failed to update preview language: {error}");
    }
    if let Err(error) = app.emit("language-changed", state()) {
        eprintln!("Failed to broadcast language: {error}");
    }
}

pub(super) fn refresh_system(app: &tauri::AppHandle) {
    let language = state();
    let system_chinese = language.system == "zh-CN";
    if SYSTEM_CHINESE.swap(system_chinese, Ordering::Relaxed) != system_chinese {
        CHINESE.store(language.resolved == "zh-CN", Ordering::Relaxed);
        changed(app);
    }
}

pub(crate) fn tr(source: &str) -> &str {
    if chinese() {
        source
    } else {
        english(source)
    }
}

fn english(source: &str) -> &str {
    match source {
        "无效的语言设置" => "Invalid language setting",
        "打开剪贴板" => "Open clipboard",
        "退出" => "Quit",
        "选择保存目录" => "Choose a destination folder",
        "导出（ZIP 未加密）" => "Export (unencrypted ZIP)",
        "选择 TP 导出包" => "Choose a TP export archive",
        "TP 导出 ZIP" => "TP export ZIP",
        "TP 大图预览" => "TP image preview",
        "TP 视频首帧预览" => "TP video frame preview",
        "视频首帧" => "Video first frame",
        "首帧预览不可用" => "First frame preview unavailable",
        "预览图不可用" => "Image preview unavailable",
        "文件已不存在，无法放回剪贴板" => "The file no longer exists and cannot be copied to the clipboard",
        "不支持的记录类型" => "Unsupported record type",
        "文件路径不可用" => "File path unavailable",
        "记录不存在" => "Record not found",
        "该记录不支持文件操作" => "This record does not support file operations",
        "图片副本不可用" => "Image copy unavailable",
        "目录记录不支持此操作" => "This operation is not supported for folders",
        "文件副本不可用" => "File copy unavailable",
        "文件名不可用" => "File name unavailable",
        "目标目录中已有同名文件，未保存任何文件" => "The destination contains a file with the same name. No files were saved",
        "未能读取系统字体，请重试" => "Could not read system fonts. Please try again",
        "读取系统字体需要在主线程执行" => "System fonts must be read on the main thread",
        "文件超过 1 MB，无法预览" => "This file exceeds 1 MB and cannot be previewed",
        "文件内容不是可预览的文本" => "This file does not contain previewable text",
        "无效的文件序号" => "Invalid file index",
        "此记录没有图片预览" => "This record has no image preview",
        "无效的预览类型" => "Invalid preview type",
        "无效的预览序号" => "Invalid preview index",
        "缩略图只用于首个文件" => "Thumbnails are only available for the first file",
        "字号须为 10 至 20 之间的整数" => "Font size must be an integer between 10 and 20",
        "无效的主题配色" => "Invalid theme",
        "无效的字体名称" => "Invalid font name",
        "重命名快捷键无效或与其他操作冲突" => "The rename shortcut is invalid or conflicts with another action",
        "设置值超出允许范围" => "Settings are outside the allowed range",
        "新数据目录非空，停止迁移以避免覆盖已有文件" => "Migration stopped because the new data folder is not empty",
        "数据目录没有父目录" => "The data folder has no parent folder",
        "附件路径包含非法相对路径" => "An attachment contains an invalid relative path",
        "迁移后的数据库完整性检查失败" => "The migrated database failed its integrity check",
        "面板窗口配置不存在" => "Panel window configuration not found",
        "面板窗口不存在" => "Panel window not found",
        "开发测试仅在开发构建中可用" => "Developer tests are only available in development builds",
        "请先关闭后台常驻再测试" => "Turn off background residency before testing",
        "面板尚未就绪" => "The panel is not ready",
        "预览需要主线程" => "The preview requires the main thread",
        "无法确定当前屏幕" => "Could not determine the current screen",
        "无法读取图片" => "Could not read the image",
        "无法注册预览手势" => "Could not register preview gestures",
        "无效的预览窗口" => "Invalid preview window",
        "预览窗口配置不存在" => "Preview window configuration not found",
        "预览窗口不存在" => "Preview window not found",
        "无效的预览图片" => "Invalid preview image",
        "预览加载已取消" => "Preview loading was cancelled",
        "记录标题最多 100 个字符，且不能换行" => "Record titles must be at most 100 characters and cannot contain line breaks",
        "分组名称无效" => "Invalid group name",
        "分组已变化，请重试" => "Groups have changed. Please try again",
        "分组不存在" => "Group not found",
        "面板位置或尺寸无效" => "Invalid panel position or size",
        "恢复回退尚未完成，数据操作已暂停，请退出并重新打开 TP 自动恢复" => "Restore rollback is incomplete. Data operations are paused. Restart TP to recover automatically",
        "附件路径无效" => "Invalid attachment path",
        "附件名称无法在 Windows 和 macOS 间迁移" => "This attachment name cannot be transferred between Windows and macOS",
        "仍有文件副本待保存，请稍后重试" => "Some file copies are still being saved. Please try again shortly",
        "记录标识无效，无法导出" => "Invalid record ID. Export could not continue",
        "文件副本位于应用数据目录之外" => "The file copy is outside the application data folder",
        "文件副本不属于该记录" => "The file copy does not belong to this record",
        "记录清单超过 64 MiB，未生成导出包" => "The record manifest exceeds 64 MiB. No archive was created",
        "不支持此 TP 导出包的版本" => "This TP export version is not supported",
        "导出包分组信息无效" => "Invalid group information in the archive",
        "导出包记录信息无效" => "Invalid record information in the archive",
        "导出包附件路径或校验信息无效" => "Invalid attachment paths or checksums in the archive",
        "导出包文件副本路径无效" => "Invalid file copy path in the archive",
        "导出包缺少 records.json，无法导入" => "The archive is missing records.json and cannot be imported",
        "记录清单过大" => "The record manifest is too large",
        "导出包缺少浏览目录或包含清单之外的文件" => "The archive is missing its browsing page or contains files not listed in its manifest",
        "导出包包含重复路径" => "The archive contains duplicate paths",
        "导出包包含不安全路径或链接" => "The archive contains unsafe paths or links",
        "导出包附件未列入清单" => "An archive attachment is not listed in the manifest",
        "导出包附件名称不规范" => "Invalid archive attachment name",
        "导出包附件大小或类型不匹配" => "Archive attachment size or type does not match",
        "附件过大" => "The attachment is too large",
        "附件解压大小超过清单" => "The extracted attachment exceeds its declared size",
        "导出包附件校验失败，未恢复任何记录" => "Attachment verification failed. No records were restored",
        "导出包记录数量过多" => "The archive contains too many records",
        "恢复日志路径无效" => "Invalid restore log path",
        "恢复中断，原数据库副本不存在；请保留数据目录以便排查" => "Restore was interrupted and the original database copy is missing. Keep the data folder for investigation",
        "恢复目录无效" => "Invalid restore folder",
        "上次恢复尚未完成，请重启 TP 后重试" => "The previous restore is incomplete. Restart TP and try again",
        "保存路径无效" => "Invalid destination path",
        "请选择应用数据目录之外的位置保存" => "Choose a destination outside the application data folder",
        "导入预览已失效，请重新选择备份" => "The import preview has expired. Select the backup again",
        "无法读取设置" => "Could not read settings",
        "无法读取面板状态" => "Could not read panel state",
        "图片位图数据不完整" => "Incomplete image bitmap data",
        "注册 PNG 剪贴板格式失败" => "Could not register the PNG clipboard format",
        "无法监听面板边框双击" => "Could not listen for double-clicks on the panel border",
        "无法注册 HTML 剪贴板格式" => "Could not register the HTML clipboard format",
        "无法注册 RTF 剪贴板格式" => "Could not register the RTF clipboard format",
        "HTML 剪贴板偏移无效，保留原始复制内容" => "Invalid HTML clipboard offsets; original copied content was preserved",
        "无法读取剪贴板格式" => "Could not read clipboard formats",
        "无法保留剪贴板格式" => "Could not preserve clipboard formats",
        "写回剪贴板格式失败" => "Could not write clipboard formats",
        _ => source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_preferences_and_system_languages() {
        for system in ["zh-CN", "zh-TW", "zh-Hant-HK", "ZH_cn"] {
            assert_eq!(resolve("system", system), "zh-CN");
        }
        for system in ["en-US", "fr-FR", "", "zhong"] {
            assert_eq!(resolve("system", system), "en");
        }
        assert_eq!(resolve("en", "zh-CN"), "en");
        assert_eq!(resolve("zh-CN", "en-US"), "zh-CN");
    }
}

macro_rules! format_message {
    ($zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::app::i18n::chinese() { format!($zh $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}
pub(crate) use format_message;
