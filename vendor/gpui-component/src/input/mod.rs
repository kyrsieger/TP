// Modified by TP (2026-11-06): add utf16_limit; see TP-PATCHES.md.
mod blink_cursor;
mod change;
mod clear_button;
mod cursor;
mod element;
mod indent;
mod input;
mod lsp;
mod mask_pattern;
mod mode;
mod movement;
mod number_input;
mod otp_input;
pub(crate) mod popovers;
mod rope_ext;
mod search;
mod state;
mod utf16_limit;
mod text_wrapper;
mod selection;

pub(crate) use clear_button::*;
pub use cursor::*;
pub use indent::TabSize;
pub use input::*;
pub use lsp::*;
pub use mask_pattern::MaskPattern;
pub use number_input::{NumberInput, NumberInputEvent, StepAction};
pub use otp_input::*;
pub use state::*;

pub use lsp_types::Position;
pub use rope_ext::*;
pub use ropey::Rope;
