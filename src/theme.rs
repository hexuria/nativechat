use gpui::*;
use gpui_component::{Theme, ThemeRegistry};
use std::path::PathBuf;

pub fn init(cx: &mut App) {
    let theme_name = SharedString::from("macOS Classic");

    // Try to load themes from the gpui-component assets
    // First, try to watch the themes directory if it exists
    let themes_path = PathBuf::from("themes");

    if let Err(err) = ThemeRegistry::watch_dir(themes_path.clone(), cx, move |cx| {
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme);
        }
    }) {
        eprintln!(
            "Failed to watch themes directory: {}, using default theme",
            err
        );

        // Fallback: Use default theme if themes directory not found
        // The default theme should still work, but may not have cursor visible
    }
}
