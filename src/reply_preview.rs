//! Compact display quotes, independent of the raw text sent to the server or copied.

pub(crate) fn preview(text: &str, is_markdown: bool, max: usize) -> String {
    let projected = is_markdown
        .then(|| markdown::to_mdast(text, &markdown::ParseOptions::gfm()).ok())
        .flatten()
        .map(|node| visible_text(&node));
    let text = projected.as_deref().unwrap_or(text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else if max == 0 {
        String::new()
    } else {
        format!("{}…", text.chars().take(max - 1).collect::<String>())
    }
}

fn visible_text(node: &markdown::mdast::Node) -> String {
    use markdown::mdast::Node;
    match node {
        Node::Image(image) => image.alt.clone(),
        Node::ImageReference(image) => image.alt.clone(),
        Node::Break(_) | Node::ThematicBreak(_) => " ".into(),
        Node::Definition(_) | Node::FootnoteDefinition(_) => String::new(),
        _ => match node.children() {
            Some(children) => children.iter().map(visible_text).collect::<Vec<_>>().join(
                if matches!(
                    node,
                    Node::Root(_)
                        | Node::Blockquote(_)
                        | Node::List(_)
                        | Node::ListItem(_)
                        | Node::Table(_)
                        | Node::TableRow(_)
                ) {
                    " "
                } else {
                    ""
                },
            ),
            None => node.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_is_projected_before_truncation() {
        assert_eq!(
            preview("Time is **Thursday, October 1**.", true, 17),
            "Time is Thursday…"
        );
    }

    #[test]
    fn quotes_keep_visible_words_and_block_boundaries() {
        let source = "# Title\n\nOne **bold** and *soft* [link](https://example.com).\n\n- First\n- Second\n\n![Screen](image.png)";
        assert_eq!(
            preview(source, true, 200),
            "Title One bold and soft link. First Second Screen"
        );
    }

    #[test]
    fn code_and_escaped_markers_remain_literal() {
        assert_eq!(
            preview(r"`**literal**` and \*escaped\*", true, 100),
            "**literal** and *escaped*"
        );
        assert_eq!(
            preview("**plain user text**", false, 100),
            "**plain user text**"
        );
    }

    #[test]
    fn unicode_and_empty_limits_are_safe() {
        assert_eq!(preview("**你好世界**", true, 3), "你好…");
        assert_eq!(preview("hello", true, 0), "");
        assert_eq!(preview("hello", true, 1), "…");
        assert_eq!(preview("", true, 10), "");
    }
}
