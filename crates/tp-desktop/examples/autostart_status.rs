//! Read the existing TP startup state without changing any system setting.
fn main() -> Result<(), String> {
    println!("enabled={}", tp_desktop::autostart::enabled()?);
    Ok(())
}
