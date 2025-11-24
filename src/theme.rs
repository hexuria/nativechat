use gpui::*;
use ui::{Theme, ThemeRegistry};
use std::path::PathBuf;

/// Returns the path to the themes directory, which is different for dev and release builds.
fn themes_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        // In development, load themes directly from the project's `themes` directory.
        PathBuf::from("themes")
    }
    #[cfg(not(debug_assertions))]
    {
        // In release, themes are copied to the `Resources` directory of the app bundle.
        // We construct the path relative to the executable.
        let exe_path = std::env::current_exe().expect("Failed to get current executable path");
        if let Some(path) = exe_path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("Resources/themes"))
        {
            if path.exists() {
                return path;
            }
        }

        // Fallback for running from target/release (project root is 3 levels up)
        if let Some(path) = exe_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.join("themes"))
        {
            if path.exists() {
                return path;
            }
        }

        // Fallback for unexpected bundle structure or CWD
        PathBuf::from("themes")
    }
}

pub fn init(cx: &mut App) {
    let theme_name = SharedString::from("macOS Classic Light");
    let themes_path = themes_path();
    println!("[THEME] Loading themes from: {:?}", themes_path);

    if let Err(err) = ThemeRegistry::watch_dir(themes_path, cx, move |cx| {
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme);
        } else {
            log::error!(
                "Default theme '{}' not found after loading themes.",
                theme_name
            );
        }
    }) {
        log::error!("Failed to load themes, using default: {}", err);
    }
}
