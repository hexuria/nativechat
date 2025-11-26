use crate::{
    button::{Button, ButtonVariants},
    v_flex, ActiveTheme, Collapsible, Icon, IconName, Side, Sizable,
};
use gpui::{
    div, prelude::FluentBuilder, AnyElement, App, ClickEvent, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, StatefulInteractiveElement, StyleRefinement,
    Styled, Window,
};
use std::rc::Rc;

mod footer;
mod group;
mod header;
mod menu;
pub use footer::*;
pub use group::*;
pub use header::*;
pub use menu::*;

/// A Sidebar element that can contain collapsible child elements.
pub struct Sidebar<E: Collapsible + IntoElement + Into<crate::resizable::ResizablePanel> + 'static>
{
    side: Side,
    content: Vec<E>,
    /// header view
    header: Option<AnyElement>,
    /// footer view
    footer: Option<AnyElement>,
    collapsible: bool,
    collapsed: bool,
    style: StyleRefinement,
}

impl<E: Collapsible + IntoElement + Into<crate::resizable::ResizablePanel>> IntoElement
    for Sidebar<E>
{
    type Element = gpui::Component<Self>;

    fn into_element(self) -> Self::Element {
        gpui::Component::new(self)
    }
}

impl<E: Collapsible + IntoElement + Into<crate::resizable::ResizablePanel>> Sidebar<E> {
    /// Create a new Sidebar on the given [`Side`].
    pub fn new(side: Side) -> Self {
        Self {
            side,
            content: Vec::new(),
            header: None,
            footer: None,
            collapsible: true,
            collapsed: false,
            style: StyleRefinement::default(),
        }
    }

    /// Create a new Sidebar on the left side.
    pub fn left() -> Self {
        Self::new(Side::Left)
    }

    /// Create a new Sidebar on the right side.
    pub fn right() -> Self {
        Self::new(Side::Right)
    }

    /// Set the sidebar to be collapsible, default is true
    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Set the sidebar to be collapsed
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Set the header of the sidebar.
    pub fn header(mut self, header: impl IntoElement) -> Self {
        self.header = Some(header.into_any_element());
        self
    }

    /// Set the footer of the sidebar.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Add a child element to the sidebar, the child must implement `Collapsible`
    pub fn child(mut self, child: E) -> Self {
        self.content.push(child);
        self
    }

    /// Add multiple children to the sidebar, the children must implement `Collapsible`
    pub fn children(mut self, children: impl IntoIterator<Item = E>) -> Self {
        self.content.extend(children);
        self
    }
}

/// Toggle button to collapse/expand the [`Sidebar`].
#[derive(IntoElement)]
pub struct SidebarToggleButton {
    btn: Button,
    collapsed: bool,
    side: Side,
    on_click: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl SidebarToggleButton {
    fn new(side: Side) -> Self {
        Self {
            btn: Button::new("collapse").ghost().small(),
            collapsed: false,
            side,
            on_click: None,
        }
    }

    /// Create a new SidebarToggleButton on the left side.
    pub fn left() -> Self {
        Self::new(Side::Left)
    }

    /// Create a new SidebarToggleButton on the right side.
    pub fn right() -> Self {
        Self::new(Side::Right)
    }

    /// Set the side of the toggle button.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Set the collapsed state of the toggle button.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Add a click handler to the toggle button.
    pub fn on_click(
        mut self,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }
}

impl RenderOnce for SidebarToggleButton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let collapsed = self.collapsed;
        let on_click = self.on_click.clone();

        let icon = if collapsed {
            if self.side.is_left() {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelRightOpen
            }
        } else {
            if self.side.is_left() {
                IconName::PanelLeftClose
            } else {
                IconName::PanelRightClose
            }
        };

        div()
            .id("sidebar-toggle")
            .flex()
            .items_center()
            .justify_center()
            .rounded(cx.theme().radius)
            .p_2()
            .cursor_pointer()
            .hover(|this| {
                this.bg(cx.theme().sidebar_accent.opacity(0.8))
                    .text_color(cx.theme().sidebar_accent_foreground)
            })
            .text_color(cx.theme().sidebar_foreground)
            .when_some(on_click, |this, on_click| {
                this.on_click(move |ev, window, cx| {
                    on_click(ev, window, cx);
                })
            })
            .child(Icon::new(icon).size_4())
    }
}

impl<E: Collapsible + IntoElement + Into<crate::resizable::ResizablePanel>> Styled for Sidebar<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<E: Collapsible + IntoElement + Into<crate::resizable::ResizablePanel>> RenderOnce
    for Sidebar<E>
{
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        v_flex()
            .id("sidebar")
            .h_full()
            .w_full()
            .bg(cx.theme().sidebar)
            .border_color(cx.theme().sidebar_border)
            .text_color(cx.theme().sidebar_foreground)
            .when(self.side.is_left(), |this| this.border_r_1())
            .when(self.side.is_right(), |this| this.border_l_1())
            .child(
                v_flex()
                    .flex_1()
                    .gap_y_4()
                    .overflow_hidden()
                    .when_some(self.header, |this, header| {
                        this.child(
                            div()
                                .id("sidebar-header")
                                .flex_shrink_0()
                                .h_12()
                                .flex()
                                .items_center()
                                .child(header),
                        )
                    })
                    .child(
                        div().flex_1().overflow_hidden().child(
                            crate::resizable::v_resizable("sidebar-content").children(
                                self.content
                                    .into_iter()
                                    .map(|c| c.collapsed(self.collapsed).into()),
                            ),
                        ),
                    )
                    .when_some(self.footer, |this, footer| {
                        this.child(div().id("sidebar-footer").flex_shrink_0().child(footer))
                    }),
            )
    }
}
