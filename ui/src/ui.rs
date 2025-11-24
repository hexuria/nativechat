// Main library file for the ui crate
// Actual source code extracted from gpui-component

pub mod colors;
pub mod components;
pub mod root;
pub mod scroll;
pub mod styled;
pub mod theme;

// Support modules
pub mod actions;
pub mod animation;
pub mod dialog;
pub mod global_state;
pub mod highlighter;
pub mod history;
pub mod kbd;
pub mod list;
pub mod menu;
pub mod notification;
pub mod sheet;
pub mod skeleton;
pub mod spinner;
pub mod text;
pub mod title_bar;
pub mod virtual_list;
pub mod window_border;

// Re-export commonly used items
pub use components::*;
pub use highlighter::HighlightTheme;
pub use root::{root::WindowExt, Root}; // WindowExt is in root::root module
pub use styled::StyledExt;
pub use theme::Colorize; // Re-export from theme where it's properly defined
pub use theme::{
    ActiveTheme, Theme, ThemeColor, ThemeConfig, ThemeMode, ThemeRegistry, ThemeSet,
    DEFAULT_THEME_COLORS,
};

mod index_path;
pub use index_path::*;

// Re-export color functions from colors module
pub use colors::*;

// Re-export commonly needed support types
pub use input::RopeExt;
pub use virtual_list::{v_virtual_list, VirtualListScrollHandle};

// Re-export common traits from styled
pub use styled::{
    AxisExt, Collapsible, Disableable, FocusableExt, LengthExt, PixelsExt, Placement, Selectable,
    Side, Sizable, Size, StyleSized,
};

// Stub for InteractiveElementExt (used by title_bar but not needed for NativeChat)
pub use gpui::InteractiveElement as InteractiveElementExt;

// Stub for rust-i18n - used by dialog, list, and input components
// In a real setup, you'd configure rust-i18n with locale files
#[macro_export]
macro_rules! _rust_i18n_t {
    ($key:expr) => {{
        // Return hardcoded English strings for common keys
        match $key {
            "Dialog.ok" => "OK",
            "Dialog.cancel" => "Cancel",
            _ => $key, // Fallback to key itself
        }
    }};
}

// Layout helpers
use gpui::{div, Div};

#[inline(always)]
pub fn h_flex() -> Div {
    div().h_flex()
}

#[inline(always)]
pub fn v_flex() -> Div {
    div().v_flex()
}
