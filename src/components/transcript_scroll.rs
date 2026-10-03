//! How the transcript keeps to its newest row, and when it lets go of it.
//!
//! While the person is at the newest row the transcript follows it: a reply that grows, a step
//! that arrives or a picture that loads, and the newest row stays in view. The whole question is
//! when they have left, and GPUI's list answers it by distance, in two places, both about the
//! same pixel. The scroll mask hands every wheel step to the list as a scrollbar position, and
//! the list takes a position within a pixel of the bottom for a drag to the end: it follows
//! again and snaps back to the bottom (`ListState::set_offset_from_scrollbar`). And every layout
//! of a list that has stopped following starts it again once it finds itself within a pixel of
//! the bottom. A trackpad moved slowly sends steps smaller than that pixel, so each one was undone
//! before the next arrived, and the person reading back was held at the newest row by the very
//! gesture meant to take them away from it. The redraws that never stop (the computer pane's
//! poll, a reply streaming in) then took back anyone who had got less than a pixel away.
//!
//! So whether the transcript follows is the person's decision, not the list's. [`Tail`] lets go
//! of the newest row at any step toward older messages, however small, and takes it back only
//! when the person comes back to it: a step that reaches the very bottom, a drag of the scrollbar
//! to its end, the jump to the latest, or a message of their own (see `chat.rs`). Nothing else
//! takes it back: no redraw, no row that arrives, no row measured again. While it is let go the
//! list is in GPUI's [`FollowMode::Normal`], which has no idea of a bottom at all, so nothing
//! inside GPUI can take it back either.

use std::ops::Range;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::{ScrollableElement as _, ScrollableMask};
use gpui_kit::component::{ActiveTheme, IconName, StyledExt as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// How much of the thread is laid out beyond the pane at either end, so rows do not pop in as
/// they scroll into view. The same as gpui-component's `MessageScroller`, which this replaces.
const OVERDRAW: Pixels = px(400.);

/// How near the end of its track the scrollbar's thumb has to be for a drag to count as taking
/// the list to its newest row. It is GPUI's own measure for a drag, kept so that dragging the
/// thumb to the bottom takes the newest row back exactly as it always has. A drag is the person
/// putting the thumb somewhere, so a distance is the right question there; it never is for the
/// wheel, which says only which way the person is going.
const DRAG_END_SLOP: f32 = 1.0;

/// Whether the transcript keeps its newest row in view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tail {
    /// At the newest row, and staying there as the thread grows.
    #[default]
    Following,
    /// The person went back up the thread, and the transcript stays where they put it.
    Detached,
}

/// Where the list stands as the wheel sees it, in pixels: how far it is scrolled from the top of
/// the thread, and how far it could be scrolled, which is the very bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reach {
    pub(crate) scrolled: f32,
    pub(crate) max: f32,
}

/// What a frame knows about the list when it looks for what the scrollbar did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Frame {
    /// GPUI's list is still following its tail. While the transcript follows, the list stops
    /// only when the person moves it up some way the wheel listener did not hear: the
    /// scrollbar's thumb dragged off the bottom, or a step the list took by itself.
    pub(crate) list_following: bool,
    /// The thread is taller than the pane, so the list has anywhere to go at all.
    pub(crate) can_scroll: bool,
    /// The person has hold of the scrollbar's thumb, or had at the last frame.
    pub(crate) dragged: bool,
    /// The thumb is at the end of its track, by GPUI's own measure for a drag.
    pub(crate) thumb_at_end: bool,
}

impl Tail {
    /// What a wheel or trackpad step does, decided before the list moves by it. `dy` is the step
    /// as the list applies it: above zero toward older messages, below zero toward newer ones.
    pub(crate) fn after_wheel(self, dy: f32, reach: Reach) -> Self {
        let max = reach.max.max(0.0);
        // The scroll mask reads where the list stands clamped to where it can be, and so does
        // this: a list told to go to its end and not laid out since reads as past it.
        let scrolled = reach.scrolled.clamp(0.0, max);
        if dy > 0.0 {
            // Toward older messages. However small the step, it is the person reading back, as
            // long as there is anywhere back to go: at the top, or in a thread that fits the
            // pane, the step moves nothing and so means nothing.
            if scrolled > 0.0 { Self::Detached } else { self }
        } else if dy < 0.0 && scrolled - dy >= max {
            // Toward newer ones, and far enough to reach the very bottom: the person came back.
            Self::Following
        } else {
            self
        }
    }

    /// What a frame finds the scrollbar did. The scrollbar is the other way a person moves the
    /// list, and it tells nobody: it sets the list's position directly. Dragged off the bottom,
    /// GPUI's list stops following by itself; dragged back to the end, the transcript takes the
    /// newest row back, as a drag always did. A frame nobody dragged in changes nothing, however
    /// near the bottom the list happens to be: that redraw is what pulled the person back down.
    pub(crate) fn after_frame(self, frame: Frame) -> Self {
        match self {
            // In a thread that fits the pane, a step up that could move nothing still stops
            // GPUI's following, and GPUI takes it back itself at the next layout. The person went
            // nowhere, so the transcript does not let go either.
            Self::Following if !frame.list_following && frame.can_scroll => Self::Detached,
            Self::Detached if frame.dragged && frame.thumb_at_end => Self::Following,
            _ => self,
        }
    }
}

/// The transcript's list and whether it follows its newest row. The two change together, here
/// and nowhere else, so GPUI's list is never following while the transcript has let go.
pub(crate) struct TranscriptScroll {
    list: ListState,
    tail: Tail,
    /// The wheel gesture in progress, read the way the scroll mask reads it, so a step taken
    /// here for vertical is exactly a step the mask moves the list by.
    gesture: OngoingScroll,
    /// The person had hold of the scrollbar's thumb at the last frame. A drag can end between
    /// two frames, and the frame after it still has to see where it ended.
    was_dragging: bool,
}

impl TranscriptScroll {
    /// A list of `item_count` rows, at the newest one.
    pub(crate) fn new(item_count: usize) -> Self {
        let list = ListState::new(item_count, ListAlignment::Top, OVERDRAW);
        list.set_follow_mode(FollowMode::Tail);
        Self {
            list,
            tail: Tail::Following,
            gesture: OngoingScroll::default(),
            was_dragging: false,
        }
    }

    pub(crate) fn list(&self) -> &ListState {
        &self.list
    }

    pub(crate) fn item_count(&self) -> usize {
        self.list.item_count()
    }

    pub(crate) fn is_following(&self) -> bool {
        self.tail == Tail::Following
    }

    /// Whether the person is away from the newest row with somewhere to come back to, which is
    /// when the jump to the latest is worth offering.
    pub(crate) fn is_scrolled_up(&self) -> bool {
        self.list.max_offset_for_scrollbar().y > px(0.)
            && !self.is_following()
            && !self.list.is_scrolled_to_end().unwrap_or(false)
    }

    /// The row at the top of the view, and how far into it the view begins.
    pub(crate) fn anchor(&self) -> ListOffset {
        self.list.logical_scroll_top()
    }

    /// Hear a wheel or trackpad step before the scroll mask moves the list by it. It has to be
    /// before: a step up applied to a list still following within a pixel of its bottom is the
    /// step GPUI takes back. Returns whether the transcript let go or took the newest row back.
    pub(crate) fn wheel(&mut self, event: &ScrollWheelEvent, line_height: Pixels) -> bool {
        let mut delta = event.delta.pixel_delta(line_height);
        // The mask locks a trackpad gesture to the axis it began on and then keeps only the
        // larger component. Read any other way, a step the mask turns sideways could let go of
        // the newest row here, or one it moves the list by could be missed.
        if event.delta.precise() {
            self.gesture.filter(&mut delta, event.touch_phase);
        }
        if delta.x != px(0.) && delta.y != px(0.) {
            if delta.x.abs() > delta.y.abs() {
                delta.y = px(0.);
            } else {
                delta.x = px(0.);
            }
        }
        let next = self.tail.after_wheel(f32::from(delta.y), self.reach());
        self.set_tail(next)
    }

    /// See what the scrollbar did since the last frame (see [`Tail::after_frame`]), before the
    /// list is laid out again. Returns whether the transcript let go or took the newest row back.
    pub(crate) fn sync_frame(&mut self) -> bool {
        let dragging = self.list.is_scrollbar_dragging();
        let dragged = dragging || self.was_dragging;
        self.was_dragging = dragging;
        let reach = self.reach();
        let next = self.tail.after_frame(Frame {
            list_following: self.list.is_following_tail(),
            can_scroll: reach.max > 0.0,
            dragged,
            thumb_at_end: reach.scrolled >= (reach.max - DRAG_END_SLOP).max(0.0),
        });
        self.set_tail(next)
    }

    /// Go to the newest row and stay on it: the jump to the latest, a thread opened, a message
    /// the person sent.
    pub(crate) fn follow(&mut self) {
        self.tail = Tail::Following;
        self.list.set_follow_mode(FollowMode::Tail);
        self.list.scroll_to_end();
    }

    /// Start the list over with `count` rows, at the newest one.
    pub(crate) fn reset(&mut self, count: usize) {
        self.list.reset(count);
        self.follow();
    }

    /// Replace the rows in `old_range` with `count` new ones. Every other row keeps the height it
    /// was measured at, and the view keeps its place: the row at the top of it stays there,
    /// unless it is one of the rows replaced.
    ///
    /// Returns `false`, changing nothing, when the range is not in the list.
    pub(crate) fn splice(&mut self, old_range: Range<usize>, count: usize) -> bool {
        if !self.valid_range(&old_range) {
            return false;
        }
        let neighbor = old_range.start.checked_sub(1);
        self.list.splice(old_range, count);
        // Every row but the last carries the gap to the next one, and only the last holds the
        // room above the composer, so a row whose place at the end may have changed holds a
        // stale height: the new last row, and the one just before the rows replaced.
        if let Some(last) = self.list.item_count().checked_sub(1) {
            self.list.remeasure_items(last..last + 1);
            if let Some(neighbor) = neighbor.filter(|neighbor| *neighbor != last) {
                self.list.remeasure_items(neighbor..neighbor + 1);
            }
        }
        true
    }

    /// Measure the rows in `range` again, keeping the view where it is.
    ///
    /// Returns `false`, changing nothing, when the range is not in the list.
    pub(crate) fn remeasure_items(&mut self, range: Range<usize>) -> bool {
        if !self.valid_range(&range) {
            return false;
        }
        self.list.remeasure_items(range);
        true
    }

    /// Put the row at `index` at the top of the view, as find does with a hit. That is the
    /// person going somewhere in the thread, so the transcript lets go of the newest row.
    pub(crate) fn scroll_to_item(&mut self, index: usize) -> bool {
        if index >= self.list.item_count() {
            return false;
        }
        self.set_tail(Tail::Detached);
        self.list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        true
    }

    /// Keep the view at `anchor` after the rows under it were replaced. Only while the
    /// transcript has let go: a transcript that follows is at the newest row whatever happens.
    pub(crate) fn hold(&mut self, anchor: ListOffset) {
        if self.tail == Tail::Detached {
            self.list.scroll_to(anchor);
        }
    }

    /// Where the list stands, read the way the scroll mask reads it for a wheel step.
    fn reach(&self) -> Reach {
        Reach {
            scrolled: -f32::from(self.list.scroll_px_offset_for_scrollbar().y),
            max: f32::from(self.list.max_offset_for_scrollbar().y),
        }
    }

    fn set_tail(&mut self, tail: Tail) -> bool {
        if self.tail == tail {
            return false;
        }
        self.tail = tail;
        match tail {
            // GPUI's own tail-following takes it from here: it puts the newest row in view at
            // every layout, however much the reply grows between two of them.
            Tail::Following => self.list.set_follow_mode(FollowMode::Tail),
            // GPUI's normal mode has no bottom to be near, so no layout and no scroll step can
            // put the transcript back on the newest row behind the person's back.
            Tail::Detached => self.list.set_follow_mode(FollowMode::Normal),
        }
        true
    }

    fn valid_range(&self, range: &Range<usize>) -> bool {
        range.start <= range.end && range.end <= self.list.item_count()
    }
}

/// What hears a wheel step over the transcript, with the line height a step of lines is
/// measured in, before the scroll mask moves the list by it.
type OnWheel = Box<dyn Fn(&ScrollWheelEvent, Pixels, &mut Window, &mut App)>;

/// What the jump to the latest does when it is pressed.
type OnJump = Box<dyn Fn(&mut Window, &mut App)>;

/// What draws the row at an index.
type RenderRow = Box<dyn FnMut(usize, &mut Window, &mut App) -> AnyElement>;

/// The transcript's scrolling surface: its rows with the gap between them, the scrollbar, the
/// scroll mask and the jump back to the newest row, laid out as gpui-component's
/// `MessageScroller` lays out its own, with one listener in front of the mask.
///
/// It is not `MessageScroller` because that keeps its list to itself: nothing outside it can take
/// the list off GPUI's tail-following, and whether the transcript follows has to be decided here
/// (see the module's own doc).
#[derive(IntoElement)]
pub(crate) struct TranscriptScroller {
    id: ElementId,
    list: ListState,
    scrolled_up: bool,
    jump_lift: Pixels,
    on_wheel: Option<OnWheel>,
    on_jump: Option<OnJump>,
    render_row: RenderRow,
    style: StyleRefinement,
}

impl TranscriptScroller {
    pub(crate) fn new<E: IntoElement>(
        id: impl Into<ElementId>,
        scroll: &TranscriptScroll,
        mut render_row: impl FnMut(usize, &mut Window, &mut App) -> E + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            list: scroll.list().clone(),
            scrolled_up: scroll.is_scrolled_up(),
            jump_lift: px(0.),
            on_wheel: None,
            on_jump: None,
            render_row: Box::new(move |ix, window, cx| {
                render_row(ix, window, cx).into_any_element()
            }),
            style: StyleRefinement::default(),
        }
    }

    /// Hear every wheel step over the transcript before the scroll mask moves the list by it,
    /// with the line height the mask turns a step of lines into pixels by.
    pub(crate) fn on_wheel(
        mut self,
        on_wheel: impl Fn(&ScrollWheelEvent, Pixels, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_wheel = Some(Box::new(on_wheel));
        self
    }

    /// What the jump to the latest does when it is pressed.
    pub(crate) fn on_jump(mut self, on_jump: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_jump = Some(Box::new(on_jump));
        self
    }

    /// Lift the jump to the latest this far off the scroller's floor, clear of what covers it.
    pub(crate) fn jump_lift(mut self, lift: Pixels) -> Self {
        self.jump_lift = lift;
        self
    }
}

impl Styled for TranscriptScroller {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TranscriptScroller {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            list: list_state,
            scrolled_up,
            jump_lift,
            on_wheel,
            on_jump,
            mut render_row,
            style,
        } = self;
        // Read before the rows are: the list holds its own state borrowed while it renders them,
        // so the row closure must not ask it anything.
        let rows = list(list_state.clone(), move |index, window, cx| {
            div()
                .w_full()
                .min_w_0()
                .px_3()
                // The row renderer owns spacing: actions are compact, messages are not.
                .child(render_row(index, window, cx))
                .into_any_element()
        })
        .size_full()
        .min_h_0();
        let viewport = div()
            .id((id.clone(), "viewport"))
            // Rows arrive at the end, as in a log, and a screen reader is told so.
            .role(Role::Log)
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(rows)
            .vertical_scrollbar(&list_state);
        let tokens = cx.theme().semantic_tokens();
        div()
            .id(id.clone())
            .relative()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .when_some(on_wheel, |this, on_wheel| {
                this.child(wheel_listener(on_wheel))
            })
            .child(viewport)
            // A vertical step inside the list stays in the list, and only reaches whatever
            // scrolls around it once the list is at an end.
            .child(ScrollableMask::new(Axis::Vertical, &list_state).id(id.clone()))
            .when(scrolled_up, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom(rems(1.))
                        .flex()
                        .justify_center()
                        .child(super::transcript_chrome::jump_button(
                            Button::new((id, "jump-to-latest"))
                                .debug_selector(|| "transcript-jump".into())
                                .secondary()
                                .icon(IconName::ArrowDown)
                                .tooltip("Jump to latest")
                                .rounded(cx.theme().radius_full())
                                .border_1()
                                .border_color(tokens.colors.border)
                                .bg(tokens.colors.background)
                                .text_color(tokens.colors.foreground)
                                .mb(jump_lift)
                                .when_some(on_jump, |button, on_jump| {
                                    button.on_click(move |_, window, cx| on_jump(window, cx))
                                }),
                        )),
                )
            })
            .refine_style(&style)
    }
}

/// The listener in front of the scroll mask. It is painted before the mask, so it hears a step
/// in the capture phase first, and it covers what the mask covers, which is the whole scroller:
/// its hitbox is the scroller's padding box, and like the mask's it goes quiet under anything
/// that occludes the transcript, such as the composer or an open picker.
fn wheel_listener(on_wheel: OnWheel) -> impl IntoElement {
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let line_height = window.line_height();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase.capture() && hitbox.should_handle_scroll(window) {
                    on_wheel(event, line_height, window, cx);
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::{Frame, Reach, Tail, TranscriptScroll};
    use gpui_kit::{ListOffset, px};

    /// A thread twice the height of the pane, and a frame of it redrawn with nobody at the
    /// scrollbar: the computer pane's poll, a reply streaming in, any notify.
    const MAX: f32 = 600.0;

    fn redraw(tail: Tail, scrolled: f32) -> Tail {
        tail.after_frame(Frame {
            list_following: tail == Tail::Following,
            can_scroll: true,
            dragged: false,
            thumb_at_end: scrolled >= MAX - 1.0,
        })
    }

    /// The owner's report, step by step: at the newest row, scroll up slowly, half a pixel at a
    /// time as a trackpad does at walking pace, with the window redrawn between steps. Every
    /// step lets go of the newest row, no redraw takes it back, and the view ends up where the
    /// steps took it. It used to end at the bottom, where every step had been undone.
    #[test]
    fn a_slow_scroll_up_is_never_pulled_back_down() {
        let mut tail = Tail::Following;
        let mut scrolled = MAX;
        for step in 0..8 {
            tail = tail.after_wheel(0.5, Reach { scrolled, max: MAX });
            assert_eq!(tail, Tail::Detached, "step {step} let go of the newest row");
            // The list moves by the step, unless the transcript still follows, in which case the
            // next layout puts it back at the bottom.
            scrolled = match tail {
                Tail::Following => MAX,
                Tail::Detached => scrolled - 0.5,
            };
            tail = redraw(tail, scrolled);
            assert_eq!(
                tail,
                Tail::Detached,
                "the redraw after step {step} left it alone"
            );
        }
        assert_eq!(
            scrolled,
            MAX - 4.0,
            "eight half-pixel steps are four pixels up"
        );
    }

    /// There is no step toward older messages small enough to be taken for staying put.
    #[test]
    fn any_step_toward_older_messages_lets_go_however_small() {
        for dy in [0.01, 0.5, 1.0, 1.5, 40.0] {
            let at_bottom = Reach {
                scrolled: MAX,
                max: MAX,
            };
            assert_eq!(
                Tail::Following.after_wheel(dy, at_bottom),
                Tail::Detached,
                "{dy}px"
            );
        }
    }

    /// A step that cannot move the list means nothing: at the very top, or in a thread short
    /// enough to fit the pane, where the transcript is at its newest row whatever the wheel does.
    #[test]
    fn a_step_that_cannot_move_the_list_changes_nothing() {
        let fits = Reach {
            scrolled: 0.0,
            max: 0.0,
        };
        assert_eq!(Tail::Following.after_wheel(3.0, fits), Tail::Following);
        let at_top = Reach {
            scrolled: 0.0,
            max: MAX,
        };
        assert_eq!(Tail::Detached.after_wheel(3.0, at_top), Tail::Detached);
        // Nor does a step that is all sideways by the time it reaches the list.
        let at_bottom = Reach {
            scrolled: MAX,
            max: MAX,
        };
        assert_eq!(Tail::Following.after_wheel(0.0, at_bottom), Tail::Following);
    }

    /// Coming back down takes the newest row back only once a step reaches the very bottom, not
    /// when it gets near it.
    #[test]
    fn only_a_step_that_reaches_the_bottom_takes_the_newest_row_back() {
        let near = Reach {
            scrolled: MAX - 10.0,
            max: MAX,
        };
        assert_eq!(Tail::Detached.after_wheel(-9.5, near), Tail::Detached);
        assert_eq!(Tail::Detached.after_wheel(-10.0, near), Tail::Following);
        assert_eq!(Tail::Detached.after_wheel(-40.0, near), Tail::Following);
        // Already at the bottom, having let go on the way there, any step down is the person
        // at the bottom asking for the newest row.
        let there = Reach {
            scrolled: MAX,
            max: MAX,
        };
        assert_eq!(Tail::Detached.after_wheel(-0.5, there), Tail::Following);
    }

    /// The scrollbar lets go and takes back the way a drag always did, and a frame in which
    /// nobody touched it changes nothing wherever the list is.
    #[test]
    fn the_scrollbar_lets_go_and_takes_back_only_while_it_is_dragged() {
        let dragged_off = Frame {
            list_following: false,
            can_scroll: true,
            dragged: true,
            thumb_at_end: false,
        };
        assert_eq!(Tail::Following.after_frame(dragged_off), Tail::Detached);
        let dragged_to_end = Frame {
            thumb_at_end: true,
            ..dragged_off
        };
        assert_eq!(Tail::Detached.after_frame(dragged_to_end), Tail::Following);
        let at_end_untouched = Frame {
            dragged: false,
            ..dragged_to_end
        };
        assert_eq!(Tail::Detached.after_frame(at_end_untouched), Tail::Detached);
        // A thread that fits the pane: GPUI stopped following over a step that moved nothing,
        // and takes it back itself.
        let fits = Frame {
            list_following: false,
            can_scroll: false,
            dragged: false,
            thumb_at_end: true,
        };
        assert_eq!(Tail::Following.after_frame(fits), Tail::Following);
    }

    /// The transcript and GPUI's list agree at every turn: the list follows exactly while the
    /// transcript does, so nothing inside GPUI follows a transcript that has let go.
    #[test]
    fn the_list_follows_exactly_while_the_transcript_does() {
        let mut scroll = TranscriptScroll::new(12);
        assert!(scroll.is_following() && scroll.list().is_following_tail());

        // Find puts a hit at the top of the view: the person went somewhere.
        assert!(scroll.scroll_to_item(3));
        assert!(!scroll.is_following() && !scroll.list().is_following_tail());
        assert_eq!(scroll.anchor().item_ix, 3);

        // Rows measured again, and rows replaced, leave it where it is.
        assert!(scroll.remeasure_items(11..12));
        assert!(scroll.splice(12..12, 2));
        assert!(!scroll.is_following() && !scroll.list().is_following_tail());
        assert_eq!(scroll.anchor().item_ix, 3);

        // The jump to the latest takes the newest row back.
        scroll.follow();
        assert!(scroll.is_following() && scroll.list().is_following_tail());

        // So does starting the list over, as opening another thread does.
        scroll.scroll_to_item(0);
        scroll.reset(5);
        assert!(scroll.is_following() && scroll.list().is_following_tail());
        assert_eq!(scroll.item_count(), 5);
    }

    /// Holding the view after a splice is for a transcript that has let go; one that follows is
    /// at its newest row, and a hold must not quietly stop GPUI following it.
    #[test]
    fn a_following_transcript_is_not_held_anywhere() {
        let mut scroll = TranscriptScroll::new(6);
        scroll.hold(ListOffset {
            item_ix: 2,
            offset_in_item: px(12.),
        });
        assert!(scroll.is_following() && scroll.list().is_following_tail());
    }
}
