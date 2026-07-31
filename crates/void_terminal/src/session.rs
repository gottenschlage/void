//! One PTY-backed terminal session and its GPUI interaction state.

use std::{collections::HashMap, path::PathBuf, time::Duration};

use gpui::{
    App, Context, Entity, ExternalPaths, FocusHandle, Focusable, KeyDownEvent,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render,
    ScrollWheelEvent, Subscription, Task, Window, div, prelude::*, px,
};
use task::Shell;
use terminal::{Event as TerminalEvent, Terminal, TerminalBuilder};
use theme::ActiveTheme as _;
use util::paths::PathStyle;

use crate::{
    Copy, DecreaseTerminalFontSize, IncreaseTerminalFontSize, Paste, ResetTerminalFontSize,
    TerminalId, TerminalSettings,
    settings::{
        decrease_terminal_font_size, increase_terminal_font_size, reset_terminal_font_size,
    },
    terminal_element::terminal_element,
};

pub(super) enum SessionState {
    Loading,
    Ready {
        terminal: Entity<Terminal>,
        _subscription: Subscription,
    },
    Failed(String),
}

pub(super) struct TerminalSession {
    id: TerminalId,
    pub(super) focus: FocusHandle,
    pub(super) state: SessionState,
    settings: TerminalSettings,
    pub(super) marked_text: Option<String>,
    cursor_visible: bool,
    terminal_blinking: bool,
    focused: bool,
    blink_epoch: usize,
    _spawn_task: Task<()>,
    _focus_subscriptions: [Subscription; 2],
}

impl TerminalSession {
    pub(super) fn spawn(
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
        let spawn_task = cx.spawn(async move |this, cx| {
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
            .ok();
        });
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
            _spawn_task: spawn_task,
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

    pub(super) fn label(&self, cx: &App) -> String {
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

    fn increase_font_size(
        &mut self,
        _: &IncreaseTerminalFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        increase_terminal_font_size(cx);
    }

    fn decrease_font_size(
        &mut self,
        _: &DecreaseTerminalFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        decrease_terminal_font_size(cx);
    }

    fn reset_font_size(
        &mut self,
        _: &ResetTerminalFontSize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        reset_terminal_font_size(cx);
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
                .text_color(cx.theme().status().error)
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
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
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

#[cfg(test)]
mod tests {
    use super::quote_path;

    #[cfg(unix)]
    #[test]
    fn quote_path_escapes_posix_single_quotes() {
        assert_eq!(quote_path("/tmp/it's here"), "'/tmp/it'\\''s here'");
    }

    #[cfg(windows)]
    #[test]
    fn quote_path_escapes_windows_double_quotes() {
        assert_eq!(quote_path(r#"C:\some "path""#), r#""C:\some \"path\"""#);
    }
}
