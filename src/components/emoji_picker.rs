use crate::state::AppState;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::rc::Rc;

pub const QUICK_REACTIONS: [&str; 6] = ["👍", "👎", "😂", "❤️", "🎉", "😮"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EmojiCat {
    pub label: &'static str,
    pub icon: &'static str,
    pub glyphs: &'static [&'static str],
}

pub const EMOJI_CATEGORIES: [EmojiCat; 8] = [
    EmojiCat {
        label: "Smileys & emotion",
        icon: "😀",
        glyphs: &[
            "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😊", "😇", "🙂", "🙃", "😉", "😌",
            "😍", "🥰", "😘", "😗", "😙", "😚", "😋", "😛", "😝", "😜", "🤪", "🤨", "🧐", "🤓",
            "😎", "🤩", "🥳", "😏", "😒", "😞", "😔", "😟", "😕", "🙁", "☹️", "😣", "😖", "😫",
            "😩", "🥺", "😢", "😭", "😤", "😠", "😡", "🤬", "🤯", "😳", "🥵", "🥶", "😱", "😨",
            "😰", "😥", "😓", "🤗", "🤔", "🤭", "🤫", "🤥", "😶", "😐", "😑", "😬", "🙄", "😯",
            "😦", "😧", "😮", "😲", "🥱", "😴", "🤤", "😪", "😵", "🤐", "🥴", "🤢", "🤮", "🤧",
            "😷", "🤒", "🤕", "🤑", "🤠", "😈", "👿", "👹", "👺", "🤡", "💩", "👻", "💀", "☠️",
            "👽", "👾", "🤖", "🎃", "😺", "😸", "😹", "😻", "😼", "😽", "🙀", "😿", "😾",
        ],
    },
    EmojiCat {
        label: "People",
        icon: "👋",
        glyphs: &[
            "👋", "🤚", "🖐️", "✋", "🖖", "👌", "🤌", "🤏", "✌️", "🤞", "🤟", "🤘", "🤙",
            "👈", "👉", "👆", "🖕", "👇", "☝️", "👍", "👎", "✊", "👊", "🤛", "🤜", "👏", "🙌",
            "🫶", "👐", "🤲", "🤝", "🙏", "✍️", "💅", "🤳", "💪", "🦾", "🦵", "🦶", "👂", "👃",
            "🧠", "👀", "👁️", "👅", "👄", "💋", "👶", "👧", "🧒", "👦", "👩", "🧑", "👨", "👱",
        ],
    },
    EmojiCat {
        label: "Animals & nature",
        icon: "🐶",
        glyphs: &[
            "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐸",
            "🐵", "🙈", "🙉", "🙊", "🐒", "🐔", "🐧", "🐦", "🐤", "🐣", "🐥", "🦆", "🦅", "🦉",
            "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🪱", "🐛", "🦋", "🐌", "🐞", "🐜", "🪰", "🪲",
            "🌸", "💮", "🏵️", "🌹", "🥀", "🌺", "🌻", "🌼", "🌷", "🌱", "🌲", "🌳", "🌴", "🌵",
        ],
    },
    EmojiCat {
        label: "Food & drink",
        icon: "🍕",
        glyphs: &[
            "🍏", "🍎", "🍐", "🍊", "🍋", "🍌", "🍉", "🍇", "🍓", "🫐", "🍈", "🍒", "🍑", "🥭",
            "🍍", "🥥", "🥝", "🍅", "🍆", "🥑", "🥦", "🥬", "🥒", "🌶️", "🫑", "🌽", "🥕", "🫒",
            "🍞", "🥐", "🥖", "🥨", "🧀", "🥚", "🍳", "🧈", "🥞", "🧇", "🥓", "🥩", "🍗", "🍖",
            "🌭", "🍔", "🍟", "🍕", "🫓", "🥪", "🥙", "🧆", "🌮", "🌯", "🫔", "🥗", "🥘", "🍝",
        ],
    },
    EmojiCat {
        label: "Travel & places",
        icon: "✈️",
        glyphs: &[
            "🚗", "🚕", "🚙", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚐", "🛻", "🚚", "🚛", "🚜",
            "🛵", "🏍️", "🚲", "🛴", "🛹", "🛼", "🚁", "✈️", "🛩️", "🛫", "🛬", "🚀", "🛸", "🚂",
            "🏠", "🏡", "🏢", "🏣", "🏤", "🏥", "🏦", "🏨", "🏩", "🏪", "🏫", "🏬", "🏭", "🏯",
            "🏰", "💒", "🗼", "🗽", "⛪", "🕌", "🛕", "🕍", "⛩️", "🕋", "⛲", "⛺", "🌁", "🌃",
        ],
    },
    EmojiCat {
        label: "Activities",
        icon: "⚽",
        glyphs: &[
            "⚽", "🏀", "🏈", "⚾", "🥎", "🎾", "🏐", "🏉", "🥏", "🎱", "🪀", "🏓", "🏸", "🏒",
            "🏑", "🥍", "🏏", "🪃", "🥅", "⛳", "🪁", "🏹", "🎣", "🤿", "🥊", "🥋", "🎽", "🛹",
            "🥇", "🥈", "🥉", "🏅", "🎖️", "🏆", "🎯", "🎮", "🕹️", "🎲", "🧩", "♟️", "🎭", "🎨",
            "🎬", "🎤", "🎧", "🎼", "🎹", "🥁", "🪘", "🎷", "🎺", "🪗", "🎸", "🪕", "🎻", "🎉",
        ],
    },
    EmojiCat {
        label: "Objects",
        icon: "💡",
        glyphs: &[
            "⌚", "📱", "📲", "💻", "⌨️", "🖥️", "🖨️", "🖱️", "🖲️", "🕹️", "🗜️", "💽", "💾", "💿",
            "📀", "📷", "📸", "📹", "🎥", "📽️", "🎞️", "📞", "☎️", "📟", "📠", "📺", "📻", "🎙️",
            "💡", "🔦", "🕯️", "🪔", "📕", "📖", "📗", "📘", "📙", "📚", "📓", "📒", "📃", "📜",
            "📄", "📰", "🗞️", "📑", "🔖", "🏷️", "💰", "🪙", "💴", "💵", "💶", "💷", "💸", "💳",
        ],
    },
    EmojiCat {
        label: "Symbols",
        icon: "❤️",
        glyphs: &[
            "❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❣️", "💕", "💞", "💓",
            "💗", "💖", "💘", "💝", "💟", "☮️", "✝️", "☪️", "🕉️", "☸️", "✡️", "🔯", "🕎", "☯️",
            "☦️", "🛐", "⛎", "♈", "♉", "♊", "♋", "♌", "♍", "♎", "♏", "♐", "♑", "♒", "♓",
            "✅", "❌", "❓", "❗", "💯", "🔴", "🟠", "🟡", "🟢", "🔵", "🟣", "⚫", "⚪", "🟤",
        ],
    },
];

fn pick_emoji(
    app: &Entity<AppState>,
    message_id: String,
    emoji: String,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let app = app.clone();
    move |_, _, cx| {
        cx.stop_propagation();
        app.update(cx, |state, cx| {
            state.toggle_reaction(message_id.clone(), emoji.clone(), cx);
        });
    }
}

fn glyph_btn(
    id: SharedString,
    glyph: &str,
    size: Pixels,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(size)
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(0x888888).opacity(0.14)))
        .child(div().text_size(px(18.)).child(glyph.to_string()))
        .on_click(on_click)
}

pub fn reaction_strip(
    app: Entity<AppState>,
    message_id: String,
    on_more: Rc<dyn Fn(&mut Window, &mut App)>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    h_flex()
        .id(SharedString::from(format!("react-strip-{message_id}")))
        .items_center()
        .gap(px(2.))
        .px(px(8.))
        .py(px(6.))
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(theme.border)
        .rounded(px(14.))
        .shadow_lg()
    .children(QUICK_REACTIONS.iter().map(|glyph| {
        let glyph = *glyph;
        glyph_btn(
            SharedString::from(format!("react-{message_id}-{glyph}")),
            glyph,
            px(32.),
            pick_emoji(&app, message_id.clone(), glyph.to_string()),
        )
    }))
    .child(
        div()
            .id(SharedString::from(format!("react-more-{message_id}")))
            .size(px(32.))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .border_1()
            .border_color(muted.opacity(0.35))
            .hover(|s| s.bg(rgb(0x888888).opacity(0.14)))
            .child(Icon::new(IconName::Plus).size(px(14.)).text_color(muted))
            .on_click({
                let on_more = on_more.clone();
                move |_, window, cx| {
                    cx.stop_propagation();
                    on_more(window, cx);
                }
            }),
    )
}

pub fn full_picker(
    app: Entity<AppState>,
    message_id: String,
    search: Entity<InputState>,
    query: String,
    category: usize,
    on_category: Rc<dyn Fn(usize, &mut App)>,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let hover = rgb(0x888888).opacity(0.14);
    let selected = rgb(0x888888).opacity(0.22);
    let category = category.min(EMOJI_CATEGORIES.len().saturating_sub(1));
    let cat = EMOJI_CATEGORIES[category];
    let q = query.trim().to_lowercase();
    let glyphs: Vec<&'static str> = if q.is_empty() {
        cat.glyphs.to_vec()
    } else {
        EMOJI_CATEGORIES
            .iter()
            .filter(|c| c.label.to_lowercase().contains(&q))
            .flat_map(|c| c.glyphs.iter().copied())
            .take(96)
            .collect()
    };

    v_flex()
        .id(SharedString::from(format!("emoji-full-{message_id}")))
        .w(px(320.))
        .h(px(380.))
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .border_1()
        .border_color(theme.border)
        .rounded(px(14.))
        .shadow_lg()
        .overflow_hidden()
    .child(
        h_flex()
            .w_full()
            .h(px(36.))
            .px(px(10.))
            .mt(px(8.))
            .mx(px(8.))
            .max_w(px(304.))
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(theme.muted)
            .child(Icon::new(IconName::Search).size(px(14.)).text_color(muted))
            .child(
                Input::new(&search)
                    .appearance(false)
                    .focus_bordered(false)
                    .w_full(),
            ),
    )
    .child(
        div()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(4.))
            .text_xs()
            .text_color(muted)
            .child(if q.is_empty() {
                cat.label.to_string()
            } else {
                "Search results".into()
            }),
    )
    .child(
        div()
            .id(SharedString::from(format!("emoji-grid-{message_id}")))
            .flex_1()
            .min_h_0()
            .px(px(8.))
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .children(glyphs.into_iter().map(|glyph| {
                        glyph_btn(
                            SharedString::from(format!("pick-{message_id}-{glyph}")),
                            glyph,
                            px(34.),
                            pick_emoji(&app, message_id.clone(), glyph.to_string()),
                        )
                    })),
            ),
    )
    .child(
        h_flex()
            .w_full()
            .h(px(40.))
            .px(px(6.))
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(theme.border)
            .children(EMOJI_CATEGORIES.iter().enumerate().map(|(ix, cat)| {
                let on_category = on_category.clone();
                let active = q.is_empty() && ix == category;
                div()
                    .id(SharedString::from(format!("emoji-cat-{ix}")))
                    .size(px(28.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .when(active, |this| this.bg(selected))
                    .hover(move |s| s.bg(hover))
                    .child(div().text_size(px(14.)).child(cat.icon.to_string()))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        on_category(ix, cx);
                    })
            })),
    )
}
