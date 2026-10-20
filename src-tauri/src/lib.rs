mod app;
mod clipboard_text;
mod platform;

pub fn run() {
    app::run();
}

#[cfg(test)]
mod clipboard_text_tests;
