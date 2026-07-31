//! PTY-backed terminal sessions and branch-local tab ownership.
//!
//! A [`BranchTerminalPanel`] owns every terminal process for one open branch.
//! Releasing the panel releases its Zed `terminal::Terminal` entities, whose
//! lifecycle terminates the corresponding process tree.

use std::{collections::HashMap, path::PathBuf, time::Duration};

use gpui::{
    App, Context, DragMoveEvent, Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable,
    KeyBinding, KeyDownEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, ScrollHandle, ScrollWheelEvent, Subscription, Window, actions, div,
    prelude::*, px, rgb,
};
use task::Shell;
use terminal::{
    Event as TerminalEvent, Terminal, TerminalBuilder,
    terminal_settings::{AlternateScroll, CursorShape},
};
use ui::{auto_scroll_toward_edge, move_item};
use util::paths::PathStyle;

mod terminal_element;

use terminal_element::terminal_element;

const TAB_HEIGHT: f32 = 30.;
const TAB_WIDTH: f32 = 105.;
const NEW_TERMINAL_WIDTH: f32 = 30.;

actions!(void_terminal, [Copy, Paste]);

/// Registers terminal-specific platform keybindings.
pub fn init(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-c", Copy, Some("Terminal")),
        KeyBinding::new("cmd-v", Paste, Some("Terminal")),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-c", Copy, Some("Terminal")),
        KeyBinding::new("ctrl-shift-v", Paste, Some("Terminal")),
    ]);
}

/// Code-configurable terminal presentation and process defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_shape: CursorShape,
    pub cursor_blinks: bool,
    pub cursor_color: u32,
    pub foreground: u32,
    pub max_scroll_history_lines: usize,
    pub background: Option<u32>,
    pub selection_background: u32,
    pub selection_foreground: u32,
    pub alternate_scroll: AlternateScroll,
    pub option_as_meta: bool,
    pub ansi: [u32; 16],
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono".into(),
            font_size: 13.,
            line_height: 18. / 13.,
            cursor_shape: CursorShape::Underline,
            cursor_blinks: true,
            cursor_color: 0xcccccc,
            foreground: 0xcccccc,
            max_scroll_history_lines: 10_000,
            background: None,
            selection_background: 0x264f78,
            selection_foreground: 0xffffff,
            alternate_scroll: AlternateScroll::On,
            option_as_meta: false,
            ansi: [
                0x000000, 0xcd3131, 0x0dbc79, 0xe5e510, 0x2472c8, 0xbc3fbc, 0x11a8cd, 0xe5e5e5,
                0x666666, 0xf14c4c, 0x23d18b, 0xf5f543, 0x3b8eea, 0xd670d6, 0x29b8db, 0xffffff,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TerminalId(u64);

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

enum SessionState {
    Loading,
    Ready {
        terminal: Entity<Terminal>,
        _subscription: Subscription,
    },
    Failed(String),
}

struct TerminalSession {
    id: TerminalId,
    focus: FocusHandle,
    state: SessionState,
    settings: TerminalSettings,
    marked_text: Option<String>,
    cursor_visible: bool,
    terminal_blinking: bool,
    focused: bool,
    blink_epoch: usize,
    _focus_subscriptions: [Subscription; 2],
}

impl TerminalSession {
    fn spawn(
        id: TerminalId,
        working_directory: PathBuf,
        settings: TerminalSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let focus_in = cx.on_focus_in(&focus, window, |this, _, cx| this.focus_in(cx));
        let focus_out = cx.on_focus_out(&focus, window, |this, _, _, cx| this.focus_out(cx));
        let task = TerminalBuilder::new(
            Some(working_directory),
            None,
            Shell::System,
            HashMap::default(),
            settings.cursor_shape,
            settings.alternate_scroll,
            Some(settings.max_scroll_history_lines),
            Vec::new(),
            0,
            false,
            cx.entity_id().as_u64(),
            None,
            cx,
            Vec::new(),
            PathStyle::local(),
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(builder) => {
                    let terminal = cx.new(|cx| builder.subscribe(cx));
                    let subscription =
                        cx.subscribe(&terminal, |this, _, event: &TerminalEvent, cx| {
                            match event {
                                TerminalEvent::Open(terminal::MaybeNavigationTarget::Url(url)) => {
                                    cx.open_url(url);
                                }
                                TerminalEvent::BlinkChanged(blinking) => {
                                    this.terminal_blinking = *blinking;
                                    this.restart_blink(cx);
                                }
                                _ => {}
                            }
                            cx.notify();
                        });
                    this.state = SessionState::Ready {
                        terminal,
                        _subscription: subscription,
                    };
                    cx.notify();
                }
                Err(error) => {
                    this.state = SessionState::Failed(format!("Could not start shell: {error}"));
                    cx.notify();
                }
            })
        })
        .detach();
        Self {
            id,
            focus,
            state: SessionState::Loading,
            settings,
            marked_text: None,
            cursor_visible: true,
            terminal_blinking: true,
            focused: false,
            blink_epoch: 0,
            _focus_subscriptions: [focus_in, focus_out],
        }
    }

    /// Shows the cursor steadily while unfocused, blur-paused, or blinking is
    /// off; otherwise restarts the blink cycle from a visible cursor.
    fn should_blink(&self) -> bool {
        self.focused && self.settings.cursor_blinks && self.terminal_blinking
    }

    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_epoch += 1;
        if self.should_blink() {
            self.schedule_blink(self.blink_epoch, cx);
        }
        cx.notify();
    }

    /// Schedules one blink tick. A tick only acts if `epoch` still matches
    /// `self.blink_epoch`, which lets focus changes and keystrokes invalidate
    /// any tick already in flight simply by bumping the epoch.
    fn schedule_blink(&self, epoch: usize, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| this.tick_blink(epoch, cx)).ok();
        })
        .detach();
    }

    fn tick_blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch != self.blink_epoch || !self.should_blink() {
            return;
        }
        self.cursor_visible = !self.cursor_visible;
        cx.notify();
        self.schedule_blink(epoch, cx);
    }

    /// Shows the cursor and holds it steady for 500ms, mirroring Zed's
    /// `BlinkManager::pause_blinking` so the cursor doesn't blink off mid-keystroke.
    fn pause_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_epoch += 1;
        if !self.should_blink() {
            return;
        }
        let epoch = self.blink_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| {
                if epoch == this.blink_epoch {
                    this.schedule_blink(epoch, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn focus_in(&mut self, cx: &mut Context<Self>) {
        self.focused = true;
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.read(cx).focus_in();
        }
        self.restart_blink(cx);
    }

    fn focus_out(&mut self, cx: &mut Context<Self>) {
        self.focused = false;
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, _| terminal.focus_out());
        }
        self.restart_blink(cx);
    }

    fn label(&self, cx: &App) -> String {
        match &self.state {
            SessionState::Loading => "Starting…".into(),
            SessionState::Failed(_) => "Shell error".into(),
            SessionState::Ready { terminal, .. } => terminal.read(cx).title(true),
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let SessionState::Ready { terminal, .. } = &self.state else {
            return;
        };
        let terminal = terminal.clone();
        self.pause_blink(cx);
        let handled = terminal.update(cx, |terminal, _| {
            terminal.try_keystroke(&event.keystroke, self.settings.option_as_meta)
        });
        if handled {
            cx.stop_propagation();
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, _| terminal.copy(None));
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, _| terminal.paste(&text));
        }
    }

    fn paste_paths(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        let text = paths
            .paths()
            .iter()
            .map(|path| quote_path(path.to_string_lossy().as_ref()))
            .collect::<Vec<_>>()
            .join(" ");
        let text = format!(" {text} ");
        self.focus.focus(window, cx);
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, _| terminal.paste(&text));
        }
    }

    fn scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, _| terminal.scroll_wheel(event, 1.));
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, cx| terminal.mouse_down(event, cx));
        }
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, cx| {
                terminal.mouse_move(event, cx);
                if event.pressed_button.is_some() && terminal.selection_started() {
                    terminal.mouse_drag(event, terminal.last_content().terminal_bounds.bounds, cx);
                }
            });
        }
    }

    fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, cx| terminal.mouse_up(event, cx));
        }
    }

    fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let SessionState::Ready { terminal, .. } = &self.state {
            terminal.update(cx, |terminal, cx| {
                terminal.try_modifiers_change(&event.modifiers, window, cx);
            });
        }
    }
}

impl Focusable for TerminalSession {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalSession {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.state {
            SessionState::Loading => div().child("Starting shell…").into_any_element(),
            SessionState::Failed(error) => div()
                .text_color(rgb(0xf14c4c))
                .child(error.clone())
                .child(" Close this terminal tab and open a new one to retry.")
                .into_any_element(),
            SessionState::Ready { terminal, .. } => terminal_element(
                terminal.clone(),
                cx.entity(),
                self.settings.clone(),
                self.focus.is_focused(window),
                self.cursor_visible,
            )
            .into_any_element(),
        };

        div()
            .id(("terminal-session", self.id.0))
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("Terminal")
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .on_scroll_wheel(cx.listener(Self::scroll))
            .on_drop(cx.listener(Self::paste_paths))
            .px(px(12.))
            .py(px(9.))
            .child(body)
    }
}

#[cfg(unix)]
fn quote_path(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn quote_path(path: &str) -> String {
    format!("\"{}\"", path.replace('"', "\\\""))
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

#[derive(Default)]
struct TerminalTabs {
    order: Vec<TerminalId>,
    active: Option<TerminalId>,
    next_id: u64,
}

impl TerminalTabs {
    fn insert_new(&mut self) -> TerminalId {
        self.next_id += 1;
        let id = TerminalId(self.next_id);
        self.order.insert(0, id);
        self.active = Some(id);
        id
    }

    fn select(&mut self, id: TerminalId) {
        if self.order.contains(&id) {
            self.active = Some(id);
        }
    }

    fn close(&mut self, id: TerminalId) -> Option<bool> {
        let index = self.order.iter().position(|candidate| *candidate == id)?;
        let was_active = self.active == Some(id);
        self.order.remove(index);
        if was_active {
            self.active = self
                .order
                .get(index.min(self.order.len().saturating_sub(1)))
                .copied();
        }
        Some(was_active)
    }

    /// Moves `id` to sit at `target`, live, as the drag hovers over it.
    /// No-op once it's already there.
    fn reorder(&mut self, id: TerminalId, target: usize) {
        let Some(source) = self.order.iter().position(|candidate| *candidate == id) else {
            return;
        };
        if source == target {
            return;
        }
        move_item(&mut self.order, source, target);
    }
}

impl BranchTerminalPanel {
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

    pub fn activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.order.is_empty() {
            self.new_terminal(window, cx);
        } else {
            self.focus_active(window, cx);
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_defaults_are_stable() {
        let settings = TerminalSettings::default();
        assert_eq!(settings.font_family, "JetBrains Mono");
        assert_eq!(settings.font_size, 13.);
        assert_eq!(settings.cursor_shape, CursorShape::Underline);
        assert!(settings.cursor_blinks);
        assert_eq!(settings.max_scroll_history_lines, 10_000);
    }

    #[test]
    fn quotes_dropped_paths_for_a_posix_shell() {
        assert_eq!(quote_path("/tmp/it's here"), "'/tmp/it'\\''s here'");
    }

    #[test]
    fn terminal_tabs_preserve_order_selection_and_close_fallback() {
        let mut tabs = TerminalTabs::default();
        let first = tabs.insert_new();
        let second = tabs.insert_new();
        let third = tabs.insert_new();
        assert_eq!(tabs.order, [third, second, first]);
        assert_eq!(tabs.active, Some(third));

        assert_eq!(tabs.close(first), Some(false));
        assert_eq!(tabs.active, Some(third));
        tabs.select(second);
        assert_eq!(tabs.close(second), Some(true));
        assert_eq!(tabs.active, Some(third));
        assert_eq!(tabs.close(third), Some(true));
        assert_eq!(tabs.active, None);
    }

    #[test]
    fn terminal_tabs_reorder_in_both_directions() {
        let mut tabs = TerminalTabs::default();
        let first = tabs.insert_new();
        let second = tabs.insert_new();
        let third = tabs.insert_new();

        tabs.reorder(third, 2);
        assert_eq!(tabs.order, [second, first, third]);
        tabs.reorder(third, 0);
        assert_eq!(tabs.order, [third, second, first]);
    }
}
