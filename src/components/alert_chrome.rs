//! Grok Bot attention chrome: glass fills (real alpha, not opaque slabs) and
//! black/white CTAs. Orange is the attention signal only.
//!
//! GPUI has no element `backdrop-filter`. Glass is a translucent fill over the
//! pane/chat so `theme.sidebar` / the transcript shows through, plus a warm
//! shadow that feathers the edge.

use gpui_kit::component::button::ButtonCustomVariant;
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

pub struct AttentionCtas {
    pub primary: ButtonCustomVariant,
    pub secondary: ButtonCustomVariant,
    pub tertiary: ButtonCustomVariant,
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

pub fn attention_ctas(dark: bool, cx: &App) -> AttentionCtas {
    if dark {
        AttentionCtas {
            primary: ButtonCustomVariant::new(cx)
                .color(solid(0xFFFFFF))
                .foreground(solid(0x111111))
                .hover(solid(0xF0F0F0))
                .active(solid(0xE4E4E4)),
            secondary: ButtonCustomVariant::new(cx)
                .color(wash(0xFFFFFF, 0.10))
                .foreground(solid(0xFFF5E6))
                .hover(wash(0xFFFFFF, 0.16))
                .active(wash(0xFFFFFF, 0.22)),
            tertiary: ButtonCustomVariant::new(cx)
                .foreground(wash(0xFFF5E6, 0.82))
                .hover(wash(0xFFFFFF, 0.10))
                .active(wash(0xFFFFFF, 0.16)),
        }
    } else {
        AttentionCtas {
            primary: ButtonCustomVariant::new(cx)
                .color(solid(0x000000))
                .foreground(solid(0xFFFFFF))
                .hover(solid(0x1A1A1A))
                .active(solid(0x111111)),
            secondary: ButtonCustomVariant::new(cx)
                .color(wash(0x000000, 0.06))
                .foreground(solid(0x1C1917))
                .hover(wash(0x000000, 0.10))
                .active(wash(0x000000, 0.14)),
            tertiary: ButtonCustomVariant::new(cx)
                .foreground(solid(0x57534E))
                .hover(wash(0x000000, 0.06))
                .active(wash(0x000000, 0.10)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
