//! A performant markdown renderer based on Zed's streaming approach.
//!
//! This implementation uses `pulldown-cmark` for event-based parsing instead of
//! building a full AST, which can be more efficient for large documents.

use std::ops::Range;
use std::sync::Arc;

use gpui::{
    div, px, AnyElement, App, Bounds, Element, ElementId, Entity, FocusHandle, FontStyle,
    FontWeight, GlobalElementId, HighlightStyle, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, SharedString, StrikethroughStyle, StyleRefinement, Styled, StyledText,
    TextRun, TextStyle, UnderlineStyle, Window,
};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ropey::Rope;

use crate::highlighter::SyntaxHighlighter;
use crate::{h_flex, v_flex, ActiveTheme};

/// Parse options for pulldown-cmark with GFM extensions
const PARSE_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_SMART_PUNCTUATION)
    .union(Options::ENABLE_GFM);

/// A performant markdown view using event-based parsing.
#[derive(Clone)]
pub struct MarkdownView {
    id: ElementId,
    source: SharedString,
    state: Entity<MarkdownViewState>,
    style: StyleRefinement,
}

/// Internal state for the markdown view
pub struct MarkdownViewState {
    source: SharedString,
    parsed: Option<ParsedMarkdown>,
    focus_handle: FocusHandle,
}

/// Cached parse result
#[derive(Clone)]
struct ParsedMarkdown {
    source: SharedString,
    events: Arc<[(Range<usize>, MarkdownEvent)]>,
}

/// Our own markdown event enum (static lifetime for caching)
#[derive(Clone, Debug)]
enum MarkdownEvent {
    Start(MarkdownTag),
    End(MarkdownTagEnd),
    Text,
    Code(SharedString),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
}

/// Tags for container elements
#[derive(Clone, Debug)]
enum MarkdownTag {
    Paragraph,
    Heading(u8),
    BlockQuote,
    CodeBlock(Option<SharedString>),
    List(Option<u64>),
    Item,
    Emphasis,
    Strong,
    Strikethrough,
    Link { url: SharedString },
}

/// Tag end markers
#[derive(Clone, Debug)]
enum MarkdownTagEnd {
    Paragraph,
    Heading,
    BlockQuote,
    CodeBlock,
    List,
    Item,
    Emphasis,
    Strong,
    Strikethrough,
    Link,
}

impl MarkdownViewState {
    fn new(source: SharedString, cx: &mut gpui::Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let parsed = Some(parse_markdown(&source));

        Self {
            source,
            parsed,
            focus_handle,
        }
    }

    fn update_source(&mut self, source: SharedString, cx: &mut gpui::Context<Self>) {
        if self.source == source {
            return;
        }
        self.source = source.clone();
        // For now, parse synchronously. Background parsing can be added later.
        self.parsed = Some(parse_markdown(&source));
        cx.notify();
    }
}

/// Parse markdown source into events
fn parse_markdown(source: &str) -> ParsedMarkdown {
    let mut events = Vec::new();
    let parser = Parser::new_ext(source, PARSE_OPTIONS).into_offset_iter();

    for (event, range) in parser {
        let md_event = match event {
            Event::Start(tag) => {
                let md_tag = match tag {
                    Tag::Paragraph => MarkdownTag::Paragraph,
                    Tag::Heading { level, .. } => MarkdownTag::Heading(heading_level_to_u8(level)),
                    Tag::BlockQuote(_) => MarkdownTag::BlockQuote,
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(info) => {
                                let lang = info.split_whitespace().next().unwrap_or("");
                                if lang.is_empty() {
                                    None
                                } else {
                                    Some(SharedString::from(lang.to_string()))
                                }
                            }
                            pulldown_cmark::CodeBlockKind::Indented => None,
                        };
                        MarkdownTag::CodeBlock(lang)
                    }
                    Tag::List(start) => MarkdownTag::List(start),
                    Tag::Item => MarkdownTag::Item,
                    Tag::Emphasis => MarkdownTag::Emphasis,
                    Tag::Strong => MarkdownTag::Strong,
                    Tag::Strikethrough => MarkdownTag::Strikethrough,
                    Tag::Link { dest_url, .. } => MarkdownTag::Link {
                        url: SharedString::from(dest_url.to_string()),
                    },
                    _ => continue,
                };
                MarkdownEvent::Start(md_tag)
            }
            Event::End(tag) => {
                let md_tag = match tag {
                    TagEnd::Paragraph => MarkdownTagEnd::Paragraph,
                    TagEnd::Heading(_) => MarkdownTagEnd::Heading,
                    TagEnd::BlockQuote(_) => MarkdownTagEnd::BlockQuote,
                    TagEnd::CodeBlock => MarkdownTagEnd::CodeBlock,
                    TagEnd::List(_) => MarkdownTagEnd::List,
                    TagEnd::Item => MarkdownTagEnd::Item,
                    TagEnd::Emphasis => MarkdownTagEnd::Emphasis,
                    TagEnd::Strong => MarkdownTagEnd::Strong,
                    TagEnd::Strikethrough => MarkdownTagEnd::Strikethrough,
                    TagEnd::Link => MarkdownTagEnd::Link,
                    _ => continue,
                };
                MarkdownEvent::End(md_tag)
            }
            Event::Text(_) => MarkdownEvent::Text,
            Event::Code(code) => MarkdownEvent::Code(SharedString::from(code.to_string())),
            Event::SoftBreak => MarkdownEvent::SoftBreak,
            Event::HardBreak => MarkdownEvent::HardBreak,
            Event::Rule => MarkdownEvent::Rule,
            Event::TaskListMarker(checked) => MarkdownEvent::TaskListMarker(checked),
            _ => continue,
        };
        events.push((range, md_event));
    }

    ParsedMarkdown {
        source: SharedString::from(source.to_string()),
        events: Arc::from(events),
    }
}

fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

impl MarkdownView {
    /// Create a new markdown view
    pub fn new(
        id: impl Into<ElementId>,
        source: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let id: ElementId = id.into();
        let source: SharedString = source.into();

        let state = window.use_keyed_state(
            SharedString::from(format!("{}/md-state", id)),
            cx,
            |_, cx| MarkdownViewState::new(source.clone(), cx),
        );

        // Update source if changed
        state.update(cx, |state, cx| {
            state.update_source(source.clone(), cx);
        });

        Self {
            id,
            source,
            state,
            style: StyleRefinement::default(),
        }
    }

    /// Build the element tree from parsed events
    fn build_element(
        &self,
        parsed: &ParsedMarkdown,
        _window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let mut builder = ElementBuilder::new(cx);

        for (range, event) in parsed.events.iter() {
            match event {
                MarkdownEvent::Start(tag) => match tag {
                    MarkdownTag::Paragraph => {
                        builder.start_paragraph();
                    }
                    MarkdownTag::Heading(level) => {
                        builder.start_heading(*level);
                    }
                    MarkdownTag::BlockQuote => {
                        builder.start_blockquote();
                    }
                    MarkdownTag::CodeBlock(lang) => {
                        builder.start_code_block(lang.clone());
                    }
                    MarkdownTag::List(start) => {
                        builder.start_list(*start);
                    }
                    MarkdownTag::Item => {
                        builder.start_list_item();
                    }
                    MarkdownTag::Emphasis => {
                        builder.push_style(InlineStyle::Italic);
                    }
                    MarkdownTag::Strong => {
                        builder.push_style(InlineStyle::Bold);
                    }
                    MarkdownTag::Strikethrough => {
                        builder.push_style(InlineStyle::Strikethrough);
                    }
                    MarkdownTag::Link { url } => {
                        builder.push_style(InlineStyle::Link(url.clone()));
                    }
                },
                MarkdownEvent::End(tag) => match tag {
                    MarkdownTagEnd::Paragraph => {
                        builder.end_paragraph(cx);
                    }
                    MarkdownTagEnd::Heading => {
                        builder.end_heading(cx);
                    }
                    MarkdownTagEnd::BlockQuote => {
                        builder.end_blockquote(cx);
                    }
                    MarkdownTagEnd::CodeBlock => {
                        builder.end_code_block(cx);
                    }
                    MarkdownTagEnd::List => {
                        builder.end_list();
                    }
                    MarkdownTagEnd::Item => {
                        builder.end_list_item(cx);
                    }
                    MarkdownTagEnd::Emphasis
                    | MarkdownTagEnd::Strong
                    | MarkdownTagEnd::Strikethrough
                    | MarkdownTagEnd::Link => {
                        builder.pop_style();
                    }
                },
                MarkdownEvent::Text => {
                    let text = &parsed.source[range.clone()];
                    builder.push_text(text);
                }
                MarkdownEvent::Code(code) => {
                    builder.push_inline_code(code, cx);
                }
                MarkdownEvent::SoftBreak => {
                    builder.push_text(" ");
                }
                MarkdownEvent::HardBreak => {
                    builder.push_text("\n");
                }
                MarkdownEvent::Rule => {
                    builder.push_rule(cx);
                }
                MarkdownEvent::TaskListMarker(checked) => {
                    builder.push_task_marker(*checked);
                }
            }
        }

        builder.build(cx)
    }
}

impl Styled for MarkdownView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for MarkdownView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for MarkdownView {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let parsed = self.state.read(cx).parsed.clone();

        let mut element = if let Some(parsed) = parsed {
            self.build_element(&parsed, window, cx)
        } else {
            div().into_any_element()
        };

        let layout_id = element.request_layout(window, cx);
        (layout_id, element)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        element: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        element.paint(window, cx);
    }
}

/// Inline style modifications
#[derive(Clone)]
enum InlineStyle {
    Bold,
    Italic,
    Strikethrough,
    Code,
    Link(SharedString),
}

/// A text segment with its styles
struct TextSegment {
    text: String,
    styles: Vec<InlineStyle>,
}

/// Builder for constructing the element tree
/// Uses a paragraph-based model where inline content accumulates
/// and is flushed as a single StyledText when the paragraph ends.
struct ElementBuilder {
    /// Accumulated text segments for the current inline content
    segments: Vec<TextSegment>,
    /// Current style stack for inline content
    style_stack: Vec<InlineStyle>,
    /// Block-level elements collected so far
    blocks: Vec<AnyElement>,
    /// Context stack for nested structures
    context_stack: Vec<BuilderContext>,
    /// List numbering stack
    list_stack: Vec<Option<u64>>,
    /// Current heading level (0 = not in heading)
    heading_level: u8,
    /// Whether we're in a code block
    in_code_block: bool,
    /// Code block content accumulator
    code_block_text: String,
    /// Code block language
    code_block_lang: Option<SharedString>,
}

enum BuilderContext {
    Paragraph,
    ListItem { bullet: String },
    BlockQuote,
}

impl ElementBuilder {
    fn new(_cx: &App) -> Self {
        Self {
            segments: Vec::new(),
            style_stack: Vec::new(),
            blocks: Vec::new(),
            context_stack: Vec::new(),
            list_stack: Vec::new(),
            heading_level: 0,
            in_code_block: false,
            code_block_text: String::new(),
            code_block_lang: None,
        }
    }

    fn push_text(&mut self, text: &str) {
        if self.in_code_block {
            self.code_block_text.push_str(text);
        } else {
            self.segments.push(TextSegment {
                text: text.to_string(),
                styles: self.style_stack.clone(),
            });
        }
    }

    fn push_inline_code(&mut self, text: &str, _cx: &App) {
        // Add inline code as a segment with code style
        let mut styles = self.style_stack.clone();
        styles.push(InlineStyle::Code);
        self.segments.push(TextSegment {
            text: text.to_string(),
            styles,
        });
    }

    fn push_style(&mut self, style: InlineStyle) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        self.style_stack.pop();
    }

    fn start_paragraph(&mut self) {
        self.context_stack.push(BuilderContext::Paragraph);
    }

    fn end_paragraph(&mut self, cx: &App) {
        let element = self.flush_inline_content(cx);
        if let Some(el) = element {
            self.blocks.push(el);
        }
        self.context_stack.pop();
    }

    fn start_heading(&mut self, level: u8) {
        self.heading_level = level;
    }

    fn end_heading(&mut self, cx: &App) {
        let level = self.heading_level;
        self.heading_level = 0;

        if self.segments.is_empty() {
            return;
        }

        // Build raw text for heading
        let text: String = self.segments.iter().map(|s| s.text.as_str()).collect();
        self.segments.clear();

        let el = match level {
            1 => div().text_3xl().font_weight(FontWeight::BOLD),
            2 => div().text_2xl().font_weight(FontWeight::BOLD),
            3 => div().text_xl().font_weight(FontWeight::SEMIBOLD),
            4 => div().text_lg().font_weight(FontWeight::SEMIBOLD),
            5 => div().text_base().font_weight(FontWeight::MEDIUM),
            _ => div().text_sm().font_weight(FontWeight::MEDIUM),
        };

        self.blocks.push(
            el.text_color(cx.theme().foreground)
                .child(text)
                .into_any_element(),
        );
    }

    fn start_blockquote(&mut self) {
        self.context_stack.push(BuilderContext::BlockQuote);
    }

    fn end_blockquote(&mut self, cx: &App) {
        let element = self.flush_inline_content(cx);
        self.context_stack.pop();

        if let Some(content) = element {
            self.blocks.push(
                div()
                    .pl_4()
                    .border_l_4()
                    .border_color(cx.theme().border)
                    .text_color(cx.theme().muted_foreground)
                    .child(content)
                    .into_any_element(),
            );
        }
    }

    fn start_code_block(&mut self, lang: Option<SharedString>) {
        self.in_code_block = true;
        self.code_block_text.clear();
        self.code_block_lang = lang;
    }

    fn end_code_block(&mut self, cx: &App) {
        self.in_code_block = false;
        let text = std::mem::take(&mut self.code_block_text);
        let lang = self.code_block_lang.take().unwrap_or_default();

        if !text.is_empty() {
            // Generate unique ID from code content hash
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            let id = format!("md-codeblock-copy-{}", hasher.finish());

            // Apply syntax highlighting if language is specified
            let highlight_theme = &cx.theme().highlight_theme;
            let styled_content = if !lang.is_empty() {
                let mut highlighter = SyntaxHighlighter::new(&lang);
                highlighter.update(None, &Rope::from_str(&text));
                let styles = highlighter.styles(&(0..text.len()), highlight_theme);

                // Convert highlight styles to TextRuns
                let runs: Vec<TextRun> = styles
                    .iter()
                    .map(|(range, style)| {
                        let base_style = TextStyle {
                            color: style.color.unwrap_or(cx.theme().foreground),
                            font_family: cx.theme().mono_font_family.clone(),
                            font_weight: style.font_weight.unwrap_or_default(),
                            font_style: style.font_style.unwrap_or_default(),
                            ..Default::default()
                        };
                        base_style.to_run(range.len())
                    })
                    .collect();

                StyledText::new(text.clone()).with_runs(runs)
            } else {
                // No language specified - use plain monospace text
                let base_style = TextStyle {
                    color: cx.theme().foreground,
                    font_family: cx.theme().mono_font_family.clone(),
                    ..Default::default()
                };
                StyledText::new(text.clone()).with_runs(vec![base_style.to_run(text.len())])
            };

            self.blocks.push(
                div()
                    .w_full()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().secondary.opacity(0.85))
                    // Header with language label and copy button
                    .child(
                        h_flex()
                            .w_full()
                            .min_h_8()
                            .justify_between()
                            .items_center()
                            .px_3()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.5))
                            // Language label on left
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(lang),
                            )
                            // Copy button on right
                            .child(
                                crate::clipboard::Clipboard::new(gpui::ElementId::Name(id.into()))
                                    .value(text.clone()),
                            ),
                    )
                    // Code content with syntax highlighting
                    .child(div().p_3().child(styled_content).text_xs())
                    .into_any_element(),
            );
        }
    }

    fn start_list(&mut self, start: Option<u64>) {
        self.list_stack.push(start);
    }

    fn end_list(&mut self) {
        self.list_stack.pop();
    }

    fn start_list_item(&mut self) {
        let bullet = if let Some(Some(index)) = self.list_stack.last_mut() {
            let b = format!("{}.", index);
            *index += 1;
            b
        } else {
            "•".to_string()
        };
        self.context_stack.push(BuilderContext::ListItem { bullet });
    }

    fn end_list_item(&mut self, cx: &App) {
        let element = self.flush_inline_content(cx);
        let bullet = if let Some(BuilderContext::ListItem { bullet }) = self.context_stack.pop() {
            bullet
        } else {
            "•".to_string()
        };

        if let Some(content) = element {
            self.blocks.push(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(bullet),
                    )
                    .child(div().flex_1().min_w_0().child(content))
                    .into_any_element(),
            );
        }
    }

    fn push_rule(&mut self, cx: &App) {
        // Flush any pending content first
        if let Some(el) = self.flush_inline_content(cx) {
            self.blocks.push(el);
        }
        self.blocks.push(
            div()
                .w_full()
                .h(px(1.))
                .my_4()
                .bg(cx.theme().border)
                .into_any_element(),
        );
    }

    fn push_task_marker(&mut self, checked: bool) {
        let marker = if checked { "☑ " } else { "☐ " };
        self.push_text(marker);
    }

    /// Convert accumulated segments into a single StyledText element
    fn flush_inline_content(&mut self, cx: &App) -> Option<AnyElement> {
        if self.segments.is_empty() {
            return None;
        }

        let segments = std::mem::take(&mut self.segments);

        // Build the full text
        let full_text: String = segments.iter().map(|s| s.text.as_str()).collect();
        if full_text.is_empty() {
            return None;
        }

        // Build text runs for styling
        let base_style = TextStyle {
            color: cx.theme().foreground,
            ..Default::default()
        };
        let mut runs: Vec<TextRun> = Vec::new();
        let mut offset = 0;

        for segment in &segments {
            let len = segment.text.len();
            if len == 0 {
                continue;
            }

            let mut style = base_style.clone();

            // Apply inline styles
            for s in &segment.styles {
                match s {
                    InlineStyle::Bold => {
                        style.font_weight = FontWeight::BOLD;
                    }
                    InlineStyle::Italic => {
                        style.font_style = FontStyle::Italic;
                    }
                    InlineStyle::Strikethrough => {
                        style.strikethrough = Some(StrikethroughStyle {
                            thickness: px(1.),
                            color: Some(cx.theme().foreground),
                        });
                    }
                    InlineStyle::Code => {
                        style.font_family = cx.theme().mono_font_family.clone();
                        style.font_size = cx.theme().mono_font_size.into();
                        style.background_color = Some(cx.theme().secondary);
                    }
                    InlineStyle::Link(_) => {
                        style.color = cx.theme().link;
                        style.underline = Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(cx.theme().link),
                            ..Default::default()
                        });
                    }
                }
            }

            runs.push(style.to_run(len));
            offset += len;
        }

        let styled_text = StyledText::new(full_text).with_runs(runs);
        Some(styled_text.into_any_element())
    }

    fn build(mut self, cx: &App) -> AnyElement {
        // Flush any remaining content
        if let Some(el) = self.flush_inline_content(cx) {
            self.blocks.push(el);
        }

        if self.blocks.is_empty() {
            return div().into_any_element();
        }

        v_flex()
            .w_full()
            .gap_2()
            .children(self.blocks)
            .into_any_element()
    }
}
