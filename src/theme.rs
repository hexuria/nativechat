use gpui::*;
use gpui_component::{Theme, ThemeColor};

pub fn init(cx: &mut App) {
    let colors = ThemeColor {
        background: hsla(240. / 360., 0.1, 0.1, 1.0),
        foreground: hsla(0., 0., 0.98, 1.0),
        popover: hsla(240. / 360., 0.1, 0.1, 1.0),
        popover_foreground: hsla(0., 0., 0.98, 1.0),
        primary: hsla(210. / 360., 1.0, 0.5, 1.0),
        primary_foreground: hsla(0., 0., 1.0, 1.0),
        secondary: hsla(240. / 360., 0.1, 0.2, 1.0),
        secondary_foreground: hsla(0., 0., 0.98, 1.0),
        muted: hsla(240. / 360., 0.1, 0.2, 1.0),
        muted_foreground: hsla(240. / 360., 0.1, 0.6, 1.0),
        accent: hsla(240. / 360., 0.1, 0.2, 1.0),
        accent_foreground: hsla(0., 0., 0.98, 1.0),
        border: hsla(240. / 360., 0.1, 0.2, 1.0),
        input: hsla(240. / 360., 0.1, 0.2, 1.0),
        ring: hsla(210. / 360., 1.0, 0.5, 1.0),

        // Sidebar specific colors
        sidebar: hsla(240. / 360., 0.1, 0.08, 1.0), // Slightly darker than background
        sidebar_foreground: hsla(0., 0., 0.98, 1.0),
        sidebar_border: hsla(240. / 360., 0.1, 0.2, 1.0),
        sidebar_accent: hsla(240. / 360., 0.1, 0.15, 1.0),
        sidebar_accent_foreground: hsla(0., 0., 0.98, 1.0),
        sidebar_primary: hsla(210. / 360., 1.0, 0.5, 1.0),
        sidebar_primary_foreground: hsla(0., 0., 1.0, 1.0),

        // Scrollbar
        scrollbar: hsla(240. / 360., 0.1, 0.1, 1.0),
        scrollbar_thumb: hsla(240. / 360., 0.1, 0.2, 1.0),
        scrollbar_thumb_hover: hsla(240. / 360., 0.1, 0.3, 1.0),

        // Drag Handle
        drag_border: hsla(210. / 360., 1.0, 0.5, 1.0), // Blue when dragging

        // Other missing fields that might default to light
        tab_bar: hsla(240. / 360., 0.1, 0.1, 1.0),
        title_bar: hsla(240. / 360., 0.1, 0.1, 1.0),

        ..ThemeColor::default()
    };

    let theme = Theme {
        colors,
        ..Theme::default()
    };

    cx.set_global(theme);
}
