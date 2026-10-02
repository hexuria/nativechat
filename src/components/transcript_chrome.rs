//! Hit-testing for the controls floating over a clickable transcript.
use gpui_kit::InteractiveElement;
use gpui_kit::component::button::Button;

pub(super) fn jump_button(button: Button) -> Button {
    // Being painted last is not enough: attachment tiles handle mouse-down before
    // the button's click. Only the button's hitbox should occlude the transcript.
    button.occlude()
}

#[cfg(test)]
mod tests {
    use super::jump_button;
    use gpui_kit::component::message_scroller::{MessageScroller, MessageScrollerState};
    use gpui_kit::{
        AppContext, Context, Entity, InteractiveElement, IntoElement, Modifiers, MouseButton,
        ParentElement, Render, Styled, TestAppContext, Window, div, point, px, size,
    };
    use std::{cell::Cell, rc::Rc, time::Duration};

    struct Transcript {
        scroll: Entity<MessageScrollerState>,
        image_clicks: Rc<Cell<usize>>,
    }

    impl Render for Transcript {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.image_clicks.clone();
            div().w(px(400.)).h(px(300.)).child(
                MessageScroller::new("overlap", self.scroll.clone(), move |_, _, _| {
                    let clicks = clicks.clone();
                    // A tall attachment spans the jump button's coordinates. Its mouse-down
                    // handler has the same semantics as the production image/lightbox tile.
                    div()
                        .id("attachment")
                        .w_full()
                        .h(px(600.))
                        .on_mouse_down(MouseButton::Left, move |_, _, _| {
                            clicks.set(clicks.get() + 1);
                        })
                        .into_any_element()
                })
                .with_jump_button_transition(Duration::ZERO)
                .with_jump_button_renderer(|button| {
                    jump_button(button).debug_selector(|| "jump".into())
                }),
            )
        }
    }

    #[gpui_kit::test]
    fn jump_click_does_not_open_underlying_attachment(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let clicks = Rc::new(Cell::new(0));
        let scroll = cx.new(|cx| MessageScrollerState::new(2, cx));
        let (view, cx) = cx.add_window_view(|_, _| Transcript {
            scroll: scroll.clone(),
            image_clicks: clicks.clone(),
        });
        cx.simulate_resize(size(px(400.), px(300.)));
        scroll.update(cx, |state, cx| {
            state.scroll_to_item(0, cx);
        });
        cx.run_until_parked();
        // Move first: GPUI mouse-up click handling uses the hovered hitbox.
        cx.simulate_mouse_move(point(px(200.), px(260.)), None, Modifiers::none());
        let bounds = cx
            .debug_bounds("jump")
            .expect("visible jump-to-bottom button");
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        cx.simulate_click(bounds.center(), Modifiers::none());
        assert_eq!(clicks.get(), 0, "jump click opened the underlying image");
        assert!(
            cx.update(|_, cx| scroll.read(cx).is_following_tail()),
            "jump did not scroll to the end"
        );

        // Occlusion must be limited to the button: the attachment remains clickable.
        view.update(cx, |_, cx| cx.notify());
        cx.simulate_mouse_move(point(px(30.), px(30.)), None, Modifiers::none());
        cx.simulate_click(point(px(30.), px(30.)), Modifiers::none());
        assert_eq!(clicks.get(), 1);
    }
}
