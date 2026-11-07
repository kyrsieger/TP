//! Opt-in clipboard metadata only; never log copied text, HTML, paths or digests.
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

pub(crate) fn event(stage: &str, details: impl FnOnce() -> Value) {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    static LOCK: Mutex<()> = Mutex::new(());
    let Some(path) = PATH.get_or_init(|| {
        std::env::var_os("TP_CLIPBOARD_DIAGNOSTICS")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
    }) else {
        return;
    };
    let Ok(_guard) = LOCK.lock() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let _ = writeln!(
        file,
        "{}",
        json!({"time_ms": millis, "pid": std::process::id(), "stage": stage, "details": details()})
    );
}

pub(crate) fn text_metadata(text: &str) -> Value {
    let trailing = text.chars().rev().take_while(|c| c.is_whitespace());
    let codepoints: Vec<_> = trailing
        .clone()
        .take(16)
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    json!({"bytes": text.len(), "trailing_whitespace_count": trailing.count(), "trailing_codepoints_reversed": codepoints})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_contains_only_lengths_and_trailing_whitespace() {
        let metadata = text_metadata("private text\u{00a0} \r\n");
        assert_eq!(metadata["trailing_whitespace_count"], 4);
        assert_eq!(
            metadata["trailing_codepoints_reversed"],
            json!(["U+000A", "U+000D", "U+0020", "U+00A0"])
        );
        assert!(!metadata.to_string().contains("private"));
        let long = text_metadata(&" ".repeat(100_000));
        assert_eq!(long["trailing_whitespace_count"], 100_000);
        assert_eq!(
            long["trailing_codepoints_reversed"]
                .as_array()
                .unwrap()
                .len(),
            16
        );
    }
}
