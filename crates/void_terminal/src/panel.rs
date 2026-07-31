//! Branch-local terminal process and tab ownership.

use std::{collections::HashMap, path::PathBuf};

use gpui::{
    App, Context, DragMoveEvent, Entity, EventEmitter, Focusable, MouseButton, Render,
    ScrollHandle, Subscription, Window, div, prelude::*, px, rgb,
};
use ui::auto_scroll_toward_edge;

use crate::{TerminalId, TerminalSession, TerminalSettings, TerminalTabs};

const TAB_HEIGHT: f32 = 30.;
const TAB_WIDTH: f32 = 105.;
const NEW_TERMINAL_WIDTH: f32 = 30.;

#[derive(Clone)]
struct DraggedTerminal {
    id: TerminalId,
    label: String,
}

impl Render for DraggedTerminal {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .bg(rgb(0x252526))
            .child(self.label.clone())
    }
}

/// Persistent, lazily constructed terminal tabs for one branch worktree.
pub struct BranchTerminalPanel {
    working_directory: PathBuf,
    settings: TerminalSettings,
    sessions: HashMap<TerminalId, Entity<TerminalSession>>,
    session_observations: HashMap<TerminalId, Subscription>,
    tabs: TerminalTabs,
    tabs_scroll: ScrollHandle,
}

impl BranchTerminalPanel {
    /// Creates an empty panel that will start its first shell when activated.
    pub fn new(working_directory: PathBuf, settings: TerminalSettings) -> Self {
        Self {
            working_directory,
            settings,
            sessions: HashMap::new(),
            session_observations: HashMap::new(),
            tabs: TerminalTabs::default(),
            tabs_scroll: ScrollHandle::new(),
        }
    }

    /// Scrolls the tab bar toward the cursor's edge while a tab is being
    /// dragged, so a target scrolled out of view can still be reached.
    fn scroll_toward_drag(
        &mut self,
        event: &DragMoveEvent<DraggedTerminal>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        auto_scroll_toward_edge(&self.tabs_scroll, event.event.position, event.bounds);
        cx.notify();
    }

    /// Starts the first terminal if needed and focuses the active session.
    pub fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.order.is_empty() {
            self.new_terminal(window, cx);
        } else {
            self.focus_active(window, cx);
        }
    }

    /// Returns whether the panel has no terminal sessions.
    pub fn is_empty(&self) -> bool {
        self.tabs.order.is_empty()
    }

    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.tabs.insert_new();
        let session = cx.new(|cx| {
            TerminalSession::spawn(
                id,
                self.working_directory.clone(),
                self.settings.clone(),
                window,
                cx,
            )
        });
        let observation = cx.observe(&session, |_, _, cx| cx.notify());
        self.sessions.insert(id, session);
        self.session_observations.insert(id, observation);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.active_session(cx) {
            session.focus_handle(cx).focus(window, cx);
        }
    }

    fn active_session(&self, _: &App) -> Option<Entity<TerminalSession>> {
        self.tabs
            .active
            .and_then(|id| self.sessions.get(&id).cloned())
    }

    fn select(&mut self, id: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.select(id);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn close(&mut self, id: TerminalId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(was_active) = self.tabs.close(id) else {
            return;
        };
        self.sessions.remove(&id);
        self.session_observations.remove(&id);
        if self.tabs.order.is_empty() {
            self.new_terminal(window, cx);
            return;
        }
        if was_active {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    fn reorder(&mut self, id: TerminalId, target: usize, cx: &mut Context<Self>) {
        self.tabs.reorder(id, target);
        cx.notify();
    }
}

impl EventEmitter<()> for BranchTerminalPanel {}

impl Render for BranchTerminalPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_session(cx);
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .id("terminal-tabs")
                    .flex()
                    .h(px(TAB_HEIGHT))
                    .flex_none()
                    .overflow_x_scroll()
                    .track_scroll(&self.tabs_scroll)
                    .on_drag_move::<DraggedTerminal>(cx.listener(Self::scroll_toward_drag))
                    .border_b_1()
                    .border_color(rgb(0x2b2b2b))
                    .child(
                        div()
                            .id("new-terminal")
                            .w(px(NEW_TERMINAL_WIDTH))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    this.new_terminal(window, cx);
                                }),
                            )
                            .child("+"),
                    )
                    .children(self.tabs.order.iter().enumerate().map(|(index, id)| {
                        let id = *id;
                        let session = &self.sessions[&id];
                        let label = session.read(cx).label(cx);
                        let is_active = self.tabs.active == Some(id);
                        div()
                            .id(("terminal-tab", id.0))
                            .group("terminal-tab")
                            .flex()
                            .flex_none()
                            .w(px(TAB_WIDTH))
                            .text_xs()
                            .items_center()
                            .px_3()
                            .gap_2()
                            .border_r_1()
                            .border_color(rgb(0x2b2b2b))
                            .when(is_active, |tab| tab.bg(rgb(0x252526)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    this.select(id, window, cx);
                                }),
                            )
                            .on_drag(
                                DraggedTerminal {
                                    id,
                                    label: label.clone(),
                                },
                                |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                            )
                            .drag_over::<DraggedTerminal>(|style, _, _, _| {
                                style.border_l_2().border_color(rgb(0x3794ff))
                            })
                            .on_drop(cx.listener(move |this, dragged: &DraggedTerminal, _, cx| {
                                this.reorder(dragged.id, index, cx);
                            }))
                            .child(div().min_w_0().flex_1().truncate().child(label))
                            .child(
                                div()
                                    .id(("close-terminal", id.0))
                                    .flex()
                                    .flex_none()
                                    .size(px(16.))
                                    .items_center()
                                    .justify_center()
                                    .rounded_sm()
                                    .text_sm()
                                    .opacity(0.)
                                    .group_hover("terminal-tab", |button| button.opacity(1.))
                                    .cursor_pointer()
                                    .hover(|button| button.bg(rgb(0x3a3a3a)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.close(id, window, cx);
                                        }),
                                    )
                                    .child("×"),
                            )
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when_some(active, |body, session| body.child(session)),
            )
    }
}
