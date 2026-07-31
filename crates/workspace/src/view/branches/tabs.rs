//! Selectable, closable, draggable tabs for open Void branches.

use crate::{Branch, BranchId};
use gpui::{
    Context, DragMoveEvent, EventEmitter, MouseButton, MouseDownEvent, MouseUpEvent, Render,
    ScrollHandle, Window, div, prelude::*, px,
};
use theme::ActiveTheme;

use ui::{auto_scroll_toward_edge, icon_sized, scaled};

pub(crate) const HEADER_HEIGHT: f32 = 37.5;
const TAB_WIDTH: f32 = 165.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchSelected {
    pub branch_id: BranchId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchClosed {
    pub branch_id: BranchId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchMoved {
    pub branch_id: BranchId,
    pub target_index: usize,
}

#[derive(Clone)]
struct DraggedBranch {
    branch_id: BranchId,
    index: usize,
    label: String,
}

pub(crate) struct BranchHeader {
    branches: Vec<Branch>,
    active_branch_id: Option<BranchId>,
    tabs_scroll: ScrollHandle,
}

impl EventEmitter<BranchSelected> for BranchHeader {}
impl EventEmitter<BranchClosed> for BranchHeader {}
impl EventEmitter<BranchMoved> for BranchHeader {}

impl BranchHeader {
    pub(crate) fn new(branches: Vec<Branch>) -> Self {
        Self {
            active_branch_id: branches.first().map(|branch| branch.id),
            branches,
            tabs_scroll: ScrollHandle::new(),
        }
    }

    /// Scrolls the tab bar toward the cursor's edge while a tab is being
    /// dragged, so a target scrolled out of view can still be reached.
    fn scroll_toward_drag(
        &mut self,
        event: &DragMoveEvent<DraggedBranch>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        auto_scroll_toward_edge(&self.tabs_scroll, event.event.position, event.bounds);
        cx.notify();
    }

    pub(crate) fn sync(
        &mut self,
        branches: Vec<Branch>,
        active_branch_id: Option<BranchId>,
        cx: &mut Context<Self>,
    ) {
        if self.branches != branches || self.active_branch_id != active_branch_id {
            self.branches = branches;
            self.active_branch_id = active_branch_id;
            cx.notify();
        }
    }

    fn click_branch(
        &mut self,
        branch_id: BranchId,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(BranchSelected { branch_id });
    }

    fn stop_titlebar_drag(&mut self, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
    }

    fn close_branch(
        &mut self,
        branch_id: BranchId,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        cx.emit(BranchClosed { branch_id });
    }

    fn drop_branch(
        &mut self,
        dragged: &DraggedBranch,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(source_index) = self
            .branches
            .iter()
            .position(|branch| branch.id == dragged.branch_id)
        else {
            return;
        };
        if source_index == target_index || target_index >= self.branches.len() {
            return;
        }

        cx.emit(BranchMoved {
            branch_id: dragged.branch_id,
            target_index,
        });
    }
}

impl Render for BranchHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("branch-headers")
            .flex()
            .h(scaled(HEADER_HEIGHT))
            .flex_none()
            .overflow_x_scroll()
            .track_scroll(&self.tabs_scroll)
            .on_drag_move::<DraggedBranch>(cx.listener(Self::scroll_toward_drag))
            .children(self.branches.iter().enumerate().map(|(index, branch)| {
                let branch_id = branch.id;
                let is_active = self.active_branch_id == Some(branch_id);
                let label = format!("#{} {}", branch.number, branch.name);
                let dragged = DraggedBranch {
                    branch_id,
                    index,
                    label: label.clone(),
                };

                div()
                    .id(("branch-header", branch_id.as_i64() as u64))
                    .group("branch-tab")
                    .relative()
                    .flex()
                    .flex_none()
                    .w(scaled(TAB_WIDTH))
                    .h_full()
                    .items_center()
                    .px_3()
                    .border_r_1()
                    .border_color(cx.theme().colors().border_variant)
                    .text_color(if is_active {
                        cx.theme().colors().text
                    } else {
                        cx.theme().colors().text_muted
                    })
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::stop_titlebar_drag))
                    .when(is_active, |tab| {
                        tab.bg(cx.theme().colors().surface_background)
                    })
                    .when(!is_active, |tab| {
                        tab.hover(|tab| tab.bg(cx.theme().colors().element_hover))
                    })
                    .when(!is_active, |tab| {
                        tab.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(1.))
                                .bg(cx.theme().colors().border_variant),
                        )
                    })
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event, window, cx| {
                            this.click_branch(branch_id, event, window, cx);
                        }),
                    )
                    .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
                    .drag_over::<DraggedBranch>(move |tab, dragged, _, cx| {
                        let tab = tab.border_color(cx.theme().colors().text_accent);
                        if index < dragged.index {
                            tab.border_r_2()
                        } else if index > dragged.index {
                            tab.border_l_2()
                        } else {
                            tab
                        }
                    })
                    .on_drop(cx.listener(move |this, dragged, _, cx| {
                        this.drop_branch(dragged, index, cx);
                    }))
                    .child(div().min_w_0().flex_1().truncate().child(label))
                    .child(
                        div()
                            .id(("close-branch", branch_id.as_i64() as u64))
                            .absolute()
                            .right_1()
                            .size(scaled(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .opacity(0.)
                            .group_hover("branch-tab", |button| button.opacity(1.))
                            .hover(|button| button.bg(cx.theme().colors().element_hover))
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::stop_titlebar_drag))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, event, window, cx| {
                                    this.close_branch(branch_id, event, window, cx);
                                }),
                            )
                            .child(icon_sized(
                                "icons/x.svg",
                                12.,
                                cx.theme().colors().text_muted,
                            )),
                    )
            }))
            .child(
                div()
                    .h_full()
                    .min_w(px(1.))
                    .flex_1()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant),
            )
    }
}

impl Render for DraggedBranch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w(scaled(TAB_WIDTH))
            .h(scaled(HEADER_HEIGHT))
            .items_center()
            .px_3()
            .bg(cx.theme().colors().surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .shadow_md()
            .text_color(cx.theme().colors().text)
            .child(self.label.clone())
    }
}
