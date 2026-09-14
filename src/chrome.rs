//! OpenGrok V2 shell tokens and the layout rules that keep the left rail
//! from jumping vertically when it collapses.

pub const SIDEBAR_ROW: f32 = 54.0;
pub const SIDEBAR_GAP: f32 = 4.0;
pub const SIDEBAR_EXPANDED: f32 = 280.0;
pub const SIDEBAR_COLLAPSED: f32 = 88.0;
pub const SIDEBAR_MIN_EXPANDED: f32 = 240.0;
pub const SIDEBAR_MAX_EXPANDED: f32 = 400.0;
pub const SIDEBAR_HIDE_SNAP: f32 = 44.0;
pub const INFO_PANE_WIDTH: f32 = 320.0;
pub const AVATAR_PX: f32 = 36.0;
pub const AVATAR_TRIGGER_PX: f32 = 64.0;
pub const MASCOT_BOX_PX: f32 = 46.0;
pub const AUTO_COLLAPSE_WIDTH: f32 = 900.0;
/// Below this chat-column width, timestamps (hover and peek) are hidden —
/// Grok keeps an 82px rail, but a squeezed bubble makes the time useless.
pub const CHAT_TIMESTAMP_MIN_WIDTH: f32 = 480.0;
/// Desktop transcript + composer column. Grok uses ~690px; we match the
/// composer so bubbles line up with the input field.
pub const CHAT_CONTENT_MAX: f32 = 800.0;
pub const RAIL_HOVER: u32 = 0x777777;
pub const RAIL_HOVER_ALPHA: f32 = 0.32;

pub const AVATAR_SHAPES: [&str; 8] = [
    "blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop",
];

pub struct AvatarColor {
    pub id: &'static str,
    pub label: &'static str,
    pub swatch: u32,
    pub light: u32,
    pub dark: u32,
}

pub const AVATAR_COLORS: [AvatarColor; 11] = [
    AvatarColor { id: "black", label: "Black", swatch: 0x000000, light: 0x000000, dark: 0xFFFFFF },
    AvatarColor { id: "brown", label: "Brown", swatch: 0x936439, light: 0xA27952, dark: 0x855C36 },
    AvatarColor { id: "red", label: "Red", swatch: 0xFF263C, light: 0xFF3E51, dark: 0xE02135 },
    AvatarColor { id: "orange", label: "Orange", swatch: 0xFF6700, light: 0xFF781C, dark: 0xFF6700 },
    AvatarColor { id: "yellow", label: "Yellow", swatch: 0xFF9800, light: 0xFFAF38, dark: 0xFF9800 },
    AvatarColor { id: "green", label: "Green", swatch: 0x00C972, light: 0x00C972, dark: 0x009957 },
    AvatarColor { id: "cyan", label: "Cyan", swatch: 0x00BCA6, light: 0x1CC3B0, dark: 0x00A592 },
    AvatarColor { id: "blue", label: "Blue", swatch: 0x1084FE, light: 0x2A92FE, dark: 0x0E74E0 },
    AvatarColor { id: "violet", label: "Violet", swatch: 0x9159FE, light: 0xA97EFE, dark: 0x804EE0 },
    AvatarColor { id: "magenta", label: "Magenta", swatch: 0xFF309B, light: 0xFF5EB1, dark: 0xE02A88 },
    AvatarColor { id: "gray", label: "Gray", swatch: 0x777777, light: 0x959595, dark: 0x777777 },
];

const FALLBACK_COLORS: [&str; 10] = [
    "brown", "red", "orange", "yellow", "green", "cyan", "blue", "violet", "magenta", "gray",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponsiveCollapse {
    pub preferred: bool,
    pub was_narrow: bool,
}

impl Default for ResponsiveCollapse {
    fn default() -> Self {
        Self {
            preferred: false,
            was_narrow: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponsiveCollapseResult {
    pub next: ResponsiveCollapse,
    pub apply: Option<bool>,
}

pub fn is_narrow_viewport(width: f32) -> bool {
    width.is_finite() && width > 0.0 && width < AUTO_COLLAPSE_WIDTH
}

/// Window resized. `apply` is the collapsed value to set, or `None` to leave it.
pub fn collapse_for_width(
    state: ResponsiveCollapse,
    width: f32,
    is_collapsed: bool,
) -> ResponsiveCollapseResult {
    let narrow = is_narrow_viewport(width);
    if narrow == state.was_narrow {
        return ResponsiveCollapseResult {
            next: ResponsiveCollapse {
                preferred: if narrow { state.preferred } else { is_collapsed },
                was_narrow: narrow,
            },
            apply: None,
        };
    }
    if narrow {
        ResponsiveCollapseResult {
            next: ResponsiveCollapse {
                preferred: is_collapsed,
                was_narrow: true,
            },
            apply: Some(true),
        }
    } else {
        ResponsiveCollapseResult {
            next: ResponsiveCollapse {
                preferred: state.preferred,
                was_narrow: false,
            },
            apply: Some(state.preferred),
        }
    }
}

pub fn remember_choice(state: ResponsiveCollapse, is_collapsed: bool) -> ResponsiveCollapse {
    if state.was_narrow {
        state
    } else {
        ResponsiveCollapse {
            preferred: is_collapsed,
            was_narrow: state.was_narrow,
        }
    }
}

pub fn sidebar_width(hidden: bool, collapsed: bool, expanded: f32) -> f32 {
    if hidden {
        0.0
    } else if collapsed {
        SIDEBAR_COLLAPSED
    } else {
        expanded.clamp(SIDEBAR_MIN_EXPANDED, SIDEBAR_MAX_EXPANDED)
    }
}

pub fn chrome_floats(window_width: f32) -> bool {
    is_narrow_viewport(window_width)
}

pub fn chat_column_width(
    window_width: f32,
    hidden: bool,
    collapsed: bool,
    expanded: f32,
    right_pane_open: bool,
) -> f32 {
    if chrome_floats(window_width) {
        return window_width.max(0.0);
    }
    let left = sidebar_width(hidden, collapsed, expanded);
    let right = if right_pane_open { INFO_PANE_WIDTH } else { 0.0 };
    (window_width - left - right).max(0.0)
}

pub fn timestamps_fit(chat_width: f32) -> bool {
    chat_width >= CHAT_TIMESTAMP_MIN_WIDTH
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SidebarChrome {
    pub hidden: bool,
    pub collapsed: bool,
    pub expanded_width: f32,
}

/// Drag the left rail. OpenGrok snaps to the 88px mini at the collapsed
/// width, opens at 240px, and we hide if the drag goes past half the mini.
pub fn sidebar_from_resize(current: SidebarChrome, width: f32) -> SidebarChrome {
    if !width.is_finite() {
        return current;
    }
    if width <= SIDEBAR_HIDE_SNAP {
        return SidebarChrome {
            hidden: true,
            collapsed: current.collapsed,
            expanded_width: current.expanded_width,
        };
    }
    let expanded_width = width.clamp(SIDEBAR_MIN_EXPANDED, SIDEBAR_MAX_EXPANDED);
    if current.collapsed && width >= SIDEBAR_MIN_EXPANDED {
        return SidebarChrome {
            hidden: false,
            collapsed: false,
            expanded_width,
        };
    }
    if !current.collapsed && width <= SIDEBAR_COLLAPSED {
        return SidebarChrome {
            hidden: false,
            collapsed: true,
            expanded_width: current.expanded_width,
        };
    }
    if current.collapsed {
        return SidebarChrome {
            hidden: false,
            collapsed: true,
            expanded_width: current.expanded_width,
        };
    }
    SidebarChrome {
        hidden: false,
        collapsed: false,
        expanded_width,
    }
}

pub fn persona_shape_path(shape: &str) -> &'static str {
    match shape {
        "pebble" => "icons/persona/pebble.svg",
        "squircle" => "icons/persona/squircle.svg",
        "tablet" => "icons/persona/tablet.svg",
        "wedge" => "icons/persona/wedge.svg",
        "hex" => "icons/persona/hex.svg",
        "cloud" => "icons/persona/cloud.svg",
        "teardrop" => "icons/persona/teardrop.svg",
        _ => "icons/persona/blob.svg",
    }
}

pub fn resolve_persona_shape(agent_id: &str, shape: Option<&str>) -> &'static str {
    if let Some(shape) = shape {
        if AVATAR_SHAPES.contains(&shape) {
            return named_shape(shape);
        }
    }
    AVATAR_SHAPES[shipped_shape_hash(agent_id) as usize % AVATAR_SHAPES.len()]
}

pub fn resolve_persona_color(agent_id: &str, color: Option<&str>) -> &'static AvatarColor {
    if let Some(color) = color {
        if let Some(found) = AVATAR_COLORS.iter().find(|c| c.id == color) {
            return found;
        }
    }
    let index = shipped_color_index(agent_id) % FALLBACK_COLORS.len();
    let id = FALLBACK_COLORS[index];
    AVATAR_COLORS
        .iter()
        .find(|c| c.id == id)
        .unwrap_or(&AVATAR_COLORS[10])
}

fn named_shape(shape: &str) -> &'static str {
    AVATAR_SHAPES
        .iter()
        .copied()
        .find(|candidate| *candidate == shape)
        .unwrap_or("blob")
}

fn shipped_hash(value: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for unit in value.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

fn shipped_random_next(value: &mut u32) -> f64 {
    *value = value.wrapping_add(1_831_565_813);
    let mut next = imul(*value ^ (*value >> 15), 1 | *value);
    next = next.wrapping_add(imul(next ^ (next >> 7), 61 | next)) ^ next;
    f64::from((next ^ (next >> 14)) as u32) / 4_294_967_296.0
}

fn imul(a: u32, b: u32) -> u32 {
    a.wrapping_mul(b)
}

fn shipped_color_index(value: &str) -> usize {
    let seed = shipped_hash(value) ^ imul(1, 2_654_435_769);
    let mut state = seed ^ 2_654_435_769;
    (shipped_random_next(&mut state) * 10.0).floor() as usize
}

fn shipped_shape_hash(value: &str) -> u32 {
    let mut hash = shipped_hash(value);
    hash = imul(hash ^ (hash >> 16), 73_244_475);
    hash = imul(hash ^ (hash >> 13), 3_266_489_909);
    hash ^ (hash >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_window_collapses_and_widening_restores() {
        let mut s = ResponsiveCollapse::default();
        let r = collapse_for_width(s, 1400.0, false);
        assert_eq!(r.apply, None);
        s = r.next;
        let r = collapse_for_width(s, 720.0, false);
        assert_eq!(r.apply, Some(true));
        s = r.next;
        let r = collapse_for_width(s, 640.0, true);
        assert_eq!(r.apply, None);
        s = r.next;
        let r = collapse_for_width(s, 1400.0, true);
        assert_eq!(r.apply, Some(false));
    }

    #[test]
    fn a_chosen_rail_survives_a_narrow_trip() {
        let mut s = ResponsiveCollapse::default();
        s = collapse_for_width(s, 1400.0, false).next;
        s = remember_choice(s, true);
        let r = collapse_for_width(s, 700.0, true);
        assert_eq!(r.apply, Some(true));
        s = r.next;
        s = remember_choice(s, false);
        let r = collapse_for_width(s, 1400.0, false);
        assert_eq!(r.apply, Some(true), "the wide-screen choice was the rail");
    }

    #[test]
    fn sidebar_width_hides_then_minis_then_opens() {
        assert_eq!(sidebar_width(true, false, 280.0), 0.0);
        assert_eq!(sidebar_width(false, true, 280.0), SIDEBAR_COLLAPSED);
        assert_eq!(sidebar_width(false, false, 280.0), SIDEBAR_EXPANDED);
        assert_eq!(sidebar_width(false, false, 360.0), 360.0);
    }

    #[test]
    fn resize_snaps_to_mini_then_hide_then_open() {
        let expanded = SidebarChrome {
            hidden: false,
            collapsed: false,
            expanded_width: 280.0,
        };
        let mini = sidebar_from_resize(expanded, 80.0);
        assert!(mini.collapsed && !mini.hidden);
        assert_eq!(mini.expanded_width, 280.0);
        let hidden = sidebar_from_resize(mini, 20.0);
        assert!(hidden.hidden);
        let opened = sidebar_from_resize(mini, 260.0);
        assert!(!opened.collapsed && !opened.hidden);
        assert_eq!(opened.expanded_width, 260.0);
    }

    #[test]
    fn known_shape_wins_over_hash() {
        assert_eq!(resolve_persona_shape("cw_1", Some("hex")), "hex");
        assert_eq!(resolve_persona_shape("cw_1", Some("nope")), resolve_persona_shape("cw_1", None));
    }
}
