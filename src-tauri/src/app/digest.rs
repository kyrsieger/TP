use super::common::{err, Result};
use sha2::{Digest, Sha256};
use std::{fs, time::UNIX_EPOCH};
pub(super) fn is_hex_color(text: &str) -> bool {
    let Some(digits) = text.trim().strip_prefix('#') else {
        return false;
    };
    [3, 4, 6, 8].contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn hash(kind: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn file_digest(paths: &[String], kind: &str) -> Result<String> {
    let mut signature = String::new();
    for path in paths {
        let metadata = fs::metadata(path).map_err(err)?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_millis())
            .unwrap_or(0);
        signature.push_str(&format!("{path}\0{}\0{modified}\0", metadata.len()));
    }
    Ok(hash(kind, signature.as_bytes()))
}

#[cfg(test)]
#[path = "tests/digest.rs"]
mod tests;
