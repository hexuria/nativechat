use crate::config::Config;
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const LIGHT_THEME: &str = "macOS Classic Light";
pub const DARK_THEME: &str = "macOS Classic Dark";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThemePrefs {
    mode: String,
}

/// Returns the path to the themes directory, which is different for dev and release builds.
fn themes_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from("themes")
    }
    #[cfg(not(debug_assertions))]
    {
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

        PathBuf::from("themes")
    }
}

pub fn prefs_path() -> PathBuf {
    Config::data_dir().join("theme.json")
}

pub fn normalize_mode(mode: &str) -> &'static str {
    match mode {
        "dark" => "dark",
        "system" => "system",
        _ => "light",
    }
}

/// Sidebar / cmd-t: flip the live appearance to an explicit light or dark.
pub fn next_toggle_mode(current: &str, visually_dark: bool) -> &'static str {
    match normalize_mode(current) {
        "dark" => "light",
        "light" => "dark",
        _ => {
            if visually_dark {
                "light"
            } else {
                "dark"
            }
        }
    }
}

/// `None` follows the OS (system). Light/dark override NSAppearance so the
/// open window chrome matches the kit theme immediately.
pub fn window_appearance_override(mode: &str) -> Option<WindowAppearance> {
    match normalize_mode(mode) {
        "dark" => Some(WindowAppearance::Dark),
        "system" => None,
        _ => Some(WindowAppearance::Light),
    }
}

pub fn classic_theme_name(mode: &str, system_is_dark: bool) -> &'static str {
    match normalize_mode(mode) {
        "dark" => DARK_THEME,
        "system" => {
            if system_is_dark {
                DARK_THEME
            } else {
                LIGHT_THEME
            }
        }
        _ => LIGHT_THEME,
    }
}

pub fn load_saved_mode() -> String {
    load_saved_mode_from(&prefs_path())
}

pub fn load_saved_mode_from(path: &Path) -> String {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return "light".to_string();
    };
    parse_saved_mode(&raw)
}

fn parse_saved_mode(raw: &str) -> String {
    let raw = raw.trim();
    if let Ok(prefs) = serde_json::from_str::<ThemePrefs>(raw) {
        return normalize_mode(&prefs.mode).to_string();
    }
    match raw {
        "light" | "dark" | "system" => raw.to_string(),
        _ => "light".to_string(),
    }
}

pub fn save_mode(mode: &str) {
    save_mode_to(&prefs_path(), mode);
}

pub fn save_mode_to(path: &Path, mode: &str) {
    let prefs = ThemePrefs {
        mode: normalize_mode(mode).to_string(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&prefs) {
        let _ = std::fs::write(path, json);
    }
}

fn apply_named_theme(name: &str, cx: &mut App) {
    if !cx.has_global::<ThemeRegistry>() {
        return;
    }
    if let Some(theme) = ThemeRegistry::global(cx)
        .themes()
        .get(&SharedString::from(name))
        .cloned()
    {
        Theme::global_mut(cx).apply_config(&theme);
        Theme::sync_base(cx);
    } else {
        log::error!("Theme '{name}' not found after loading themes.");
    }
}

/// Paint the saved/chosen mode onto the live app: kit theme, native window
/// appearance, and a window refresh so the already-open frame flips now.
pub fn apply_mode(mode: &str, cx: &mut App) {
    if !cx.has_global::<Theme>() {
        return;
    }
    let mode = normalize_mode(mode);
    cx.set_window_appearance(window_appearance_override(mode));
    match mode {
        "system" => {
            Theme::sync_system_appearance(None, cx);
            let name = classic_theme_name(mode, Theme::global(cx).is_dark());
            apply_named_theme(name, cx);
        }
        "dark" => {
            apply_named_theme(DARK_THEME, cx);
            Theme::change(ThemeMode::Dark, None, cx);
        }
        _ => {
            apply_named_theme(LIGHT_THEME, cx);
            Theme::change(ThemeMode::Light, None, cx);
        }
    }
    Theme::sync_base(cx);
    cx.refresh_windows();
}

pub fn init(cx: &mut App) {
    let themes_path = themes_path();
    println!("[THEME] Loading themes from: {:?}", themes_path);

    // Apply the persisted choice immediately so a Dark pref is not stuck on
    // gpui-kit's startup Light until a new window opens.
    apply_mode(&load_saved_mode(), cx);

    if let Err(err) = ThemeRegistry::watch_dir(themes_path, cx, move |cx| {
        // Re-read disk: a toggle between init and this callback must win.
        apply_mode(&load_saved_mode(), cx);
    }) {
        log::error!("Failed to load themes, using default: {}", err);
    }
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob, for the same reason as in `alert_chrome`:
    // this module globs `gpui_kit::*`, which re-exports GPUI's `test`
    // attribute macro, and a glob shadows the prelude's built-in `#[test]`.
    use super::{
        DARK_THEME, LIGHT_THEME, WindowAppearance, classic_theme_name, load_saved_mode_from,
        next_toggle_mode, save_mode_to, window_appearance_override,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn system_is_not_hardcoded_dark() {
        assert_eq!(classic_theme_name("system", false), LIGHT_THEME);
        assert_eq!(classic_theme_name("system", true), DARK_THEME);
        assert_eq!(window_appearance_override("system"), None);
        assert_eq!(
            window_appearance_override("dark"),
            Some(WindowAppearance::Dark)
        );
        assert_eq!(
            window_appearance_override("light"),
            Some(WindowAppearance::Light)
        );
    }

    #[test]
    fn toggle_flips_light_and_dark() {
        assert_eq!(next_toggle_mode("light", false), "dark");
        assert_eq!(next_toggle_mode("dark", true), "light");
        assert_eq!(next_toggle_mode("system", true), "light");
        assert_eq!(next_toggle_mode("system", false), "dark");
    }

    #[test]
    fn prefs_round_trip_and_invalid_falls_back_to_light() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("nativechat-theme-test-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.json");

        assert_eq!(load_saved_mode_from(&path), "light");
        save_mode_to(&path, "dark");
        assert_eq!(load_saved_mode_from(&path), "dark");
        save_mode_to(&path, "system");
        assert_eq!(load_saved_mode_from(&path), "system");
        save_mode_to(&path, "nope");
        assert_eq!(load_saved_mode_from(&path), "light");
        std::fs::write(&path, "dark").unwrap();
        assert_eq!(load_saved_mode_from(&path), "dark");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
