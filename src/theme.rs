use gpui::*;
use gpui_component::{Theme, ThemeRegistry};
use std::path::PathBuf;

pub fn init(cx: &mut App) {
    let theme_name = SharedString::from("macOS Classic Light");

    // Try to load themes from the gpui-component assets
    // First, try to watch the themes directory if it exists
    let themes_path = PathBuf::from("themes");

    println!("[THEME INIT] Watching themes directory: {:?}", themes_path);

    if let Err(err) = ThemeRegistry::watch_dir(themes_path.clone(), cx, move |cx| {
        println!("[THEME INIT] Callback triggered!");
        println!(
            "[THEME INIT] Available themes: {:?}",
            ThemeRegistry::global(cx)
                .themes()
                .keys()
                .collect::<Vec<_>>()
        );

        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            println!("[THEME INIT] Applying default theme: {}", theme_name);
            Theme::global_mut(cx).apply_config(&theme);
        } else {
            println!("[THEME INIT] Default theme not found!");
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
