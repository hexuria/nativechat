//! Grok Bot attention chrome: glass fills (real alpha, not opaque slabs) and
//! black/white CTAs. Orange is the attention signal only.
//!
//! GPUI has no element `backdrop-filter`. Glass is a translucent fill over the
//! pane/chat so `theme.sidebar` / the transcript shows through, plus a warm
//! shadow that feathers the edge.
//!
//! Filled CTAs are local pills, not `ButtonVariant::Custom`. The kit mixes
//! Custom rest fill with transparent at 0.2, so "I'm done, continue" would
//! look washed until hover.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

fn wash(hex: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(hex).into();
    color.a = alpha;
    color
}

fn solid(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub struct AttentionGlass {
    pub bg: Hsla,
    pub border: Hsla,
    pub title: Hsla,
    pub body: Hsla,
    pub badge_bg: Hsla,
    pub badge_fg: Hsla,
    pub card_bg: Hsla,
    pub card_border: Hsla,
    pub preview_bg: Hsla,
}

/// Rest / hover / press fills. Primary is opaque black (light) or white (dark)
/// at rest — hover only darkens or lightens slightly.
#[derive(Clone, Copy)]
pub struct CtaFill {
    pub bg: Hsla,
    pub fg: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
}

pub struct AttentionCtas {
    pub primary: CtaFill,
    pub secondary: CtaFill,
    pub tertiary: CtaFill,
}

/// Sidebar **Needs your attention** + in-chat Computer card.
pub fn attention_glass(dark: bool) -> AttentionGlass {
    if dark {
        AttentionGlass {
            // Orange-tinted glass over the dark sidebar — not solid #382b1d.
            bg: wash(0xFF8C00, 0.16),
            border: wash(0xFFB74D, 0.22),
            title: solid(0xFFB74D),
            body: solid(0xFFF5E6),
            badge_bg: wash(0xFFB74D, 0.16),
            badge_fg: solid(0xFFB74D),
            card_bg: wash(0x2A2A2A, 0.72),
            card_border: wash(0xFFFFFF, 0.08),
            preview_bg: solid(0x121212),
        }
    } else {
        AttentionGlass {
            // Warm peach/cream WITH alpha — not opaque #FFF5E6.
            bg: wash(0xFFF5E6, 0.78),
            border: wash(0xC05621, 0.12),
            title: solid(0xC05621),
            body: solid(0x1C1917),
            badge_bg: wash(0xC05621, 0.12),
            badge_fg: solid(0xC05621),
            card_bg: wash(0xF2F2F2, 0.82),
            card_border: wash(0x000000, 0.06),
            preview_bg: solid(0xFFFFFF),
        }
    }
}

/// Soft warm lift so the glass reads as a layer, not a flat fill.
pub fn attention_shadow(dark: bool) -> Vec<BoxShadow> {
    let ink = if dark {
        wash(0xFFB74D, 0.12)
    } else {
        wash(0xC05621, 0.10)
    };
    vec![
        BoxShadow::new(px(0.), px(8.), ink).blur_radius(px(16.)),
        BoxShadow::new(px(0.), px(1.), ink).blur_radius(px(3.)),
    ]
}

pub fn attention_ctas(dark: bool) -> AttentionCtas {
    if dark {
        AttentionCtas {
            primary: CtaFill {
                bg: solid(0xFFFFFF),
                fg: solid(0x111111),
                hover: solid(0xF0F0F0),
                active: solid(0xE4E4E4),
            },
            secondary: CtaFill {
                bg: wash(0xFFFFFF, 0.10),
                fg: solid(0xFFF5E6),
                hover: wash(0xFFFFFF, 0.16),
                active: wash(0xFFFFFF, 0.22),
            },
            tertiary: CtaFill {
                bg: wash(0xFFFFFF, 0.0),
                fg: wash(0xFFF5E6, 0.82),
                hover: wash(0xFFFFFF, 0.10),
                active: wash(0xFFFFFF, 0.16),
            },
        }
    } else {
        AttentionCtas {
            primary: CtaFill {
                bg: solid(0x000000),
                fg: solid(0xFFFFFF),
                hover: solid(0x1A1A1A),
                active: solid(0x111111),
            },
            secondary: CtaFill {
                bg: wash(0x000000, 0.06),
                fg: solid(0x1C1917),
                hover: wash(0x000000, 0.10),
                active: wash(0x000000, 0.14),
            },
            tertiary: CtaFill {
                bg: wash(0x000000, 0.0),
                fg: solid(0x57534E),
                hover: wash(0x000000, 0.06),
                active: wash(0x000000, 0.10),
            },
        }
    }
}

/// Full-fill pill at rest. Do not route this through `Button::custom`.
pub fn attention_cta(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    fill: CtaFill,
    pill: bool,
    disabled: bool,
    on_click: Option<impl Fn(&mut App) + 'static>,
) -> AnyElement {
    let radius = if pill { px(999.) } else { px(8.) };
    let click = if disabled { None } else { on_click };
    div()
        .id(id)
        .h(px(24.))
        .px(px(8.))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(radius)
        .bg(fill.bg)
        .text_color(fill.fg)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .when(disabled, |this| this.opacity(0.45))
        .when(!disabled, |this| {
            this.cursor_pointer()
                .hover(|s| s.bg(fill.hover).text_color(fill.fg))
                .active(|s| s.bg(fill.active).text_color(fill.fg))
        })
        .when_some(click, |this, on_click| {
            this.on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                on_click(cx);
            })
        })
        .child(label.into())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    // Named imports, not a glob: this module globs `gpui_kit::*`, whose root
    // re-exports `gpui::*` — and that includes GPUI's own `test` attribute
    // macro. A glob shadows the prelude, so `use super::*` here would make
    // every `#[test]` below resolve to `gpui::test`, which emits a `#[test]`
    // of its own and expands until rustc runs out of recursion. Kit says as
    // much where it re-exports: "Test modules should import their Kit types
    // explicitly to avoid shadowing Rust's #[test]."
    use super::{attention_ctas, attention_glass};

    #[test]
    fn attention_glass_is_translucent() {
        let light = attention_glass(false);
        assert!(
            light.bg.a > 0.70 && light.bg.a < 0.90,
            "light sidebar glass must be peach with alpha, not opaque #FFF5E6"
        );
        let dark = attention_glass(true);
        assert!(
            dark.bg.a > 0.10 && dark.bg.a < 0.70,
            "dark sidebar glass must be translucent, not solid #382b1d"
        );
        assert!(light.card_bg.a < 1.0);
        assert!(dark.card_bg.a < 1.0);
    }

    #[test]
    fn attention_primary_is_full_strength_at_rest() {
        let light = attention_ctas(false);
        assert_eq!(
            light.primary.bg.a, 1.0,
            "light I'm done / Take over rest fill must be opaque, not kit Custom 0.2 mix"
        );
        assert!(
            light.primary.bg.l < 0.08,
            "light primary rest is a black pill, got l={}",
            light.primary.bg.l
        );
        assert!(
            light.primary.fg.l > 0.9,
            "light primary label is white, got l={}",
            light.primary.fg.l
        );
        assert!(
            light.primary.hover.l > light.primary.bg.l,
            "light hover may lighten slightly, not invent a muted rest"
        );

        let dark = attention_ctas(true);
        assert_eq!(dark.primary.bg.a, 1.0);
        assert!(
            dark.primary.bg.l > 0.9,
            "dark primary rest is a white pill, got l={}",
            dark.primary.bg.l
        );
        assert!(
            dark.primary.fg.l < 0.15,
            "dark primary label is black, got l={}",
            dark.primary.fg.l
        );
        assert!(dark.primary.hover.l < dark.primary.bg.l);
    }
}
