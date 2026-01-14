mod format;
mod inline;
pub mod markdown_view;
mod node;
mod style;
mod text_view;
mod utils;

use gpui::App;
pub use markdown_view::{strip_markdown, MarkdownView};
pub use style::*;
pub use text_view::*;

pub(crate) fn init(cx: &mut App) {
    text_view::init(cx);
}
