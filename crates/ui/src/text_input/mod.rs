//! Single-line text input entity and platform input handling.

use std::ops::Range;

use gpui::{
    AccessibleAction, App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Role, SharedString, UTF16Selection, Window, accesskit::ActionData, actions, div,
    prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation as _;

mod element;

use element::InputElement;
use theme::ActiveTheme;

actions!(
    text_input,
    [
        Backspace,
        Copy,
        Cut,
        Delete,
        Left,
        Paste,
        Right,
        SelectAll,
        SelectLeft,
        SelectRight,
    ]
);

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextSelection {
    range: Range<usize>,
    reversed: bool,
}

impl TextSelection {
    fn caret(offset: usize) -> Self {
        Self {
            range: offset..offset,
            reversed: false,
        }
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.range.start
        } else {
            self.range.end
        }
    }

    fn move_to(&mut self, offset: usize) {
        self.range = offset..offset;
        self.reversed = false;
    }

    fn select_to(&mut self, offset: usize) {
        if self.reversed {
            self.range.start = offset;
        } else {
            self.range.end = offset;
        }
        if self.range.end < self.range.start {
            self.reversed = !self.reversed;
            self.range = self.range.end..self.range.start;
        }
    }

    fn select_all(&mut self, end: usize) {
        self.range = 0..end;
        self.reversed = false;
    }
}

/// A reusable single-line text input with native platform text and IME support.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selection: TextSelection,
    marked_range: Option<Range<usize>>,
    last_layout: Option<gpui::ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextInput {
    /// Creates an empty text input using `placeholder` as its visual and accessible label.
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selection: TextSelection::caret(0),
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    /// Returns the input's current UTF-8 text.
    pub fn text(&self) -> &str {
        &self.content
    }

    /// Replaces the input's content and places the caret at the end.
    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        self.selection.move_to(self.content.len());
        self.marked_range = None;
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.range.is_empty() {
            let cursor = self.selection.cursor();
            let previous = self.previous_boundary(cursor);
            if previous == cursor {
                window.play_system_bell();
                return;
            }
            self.selection.select_to(previous);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selection.range.is_empty() {
            let cursor = self.selection.cursor();
            let next = self.next_boundary(cursor);
            if next == cursor {
                window.play_system_bell();
                return;
            }
            self.selection.select_to(next);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.range.is_empty() {
            self.previous_boundary(self.selection.cursor())
        } else {
            self.selection.range.start
        };
        self.selection.move_to(offset);
        cx.notify();
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let offset = if self.selection.range.is_empty() {
            self.next_boundary(self.selection.cursor())
        } else {
            self.selection.range.end
        };
        self.selection.move_to(offset);
        cx.notify();
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.selection
            .select_to(self.previous_boundary(self.selection.cursor()));
        cx.notify();
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.selection
            .select_to(self.next_boundary(self.selection.cursor()));
        cx.notify();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace(['\r', '\n'], " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selection.range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.copy(&Copy, window, cx);
        if !self.selection.range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection.select_all(self.content.len());
        cx.notify();
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.selection.select_to(offset);
        } else {
            self.selection.move_to(offset);
        }
        cx.notify();
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.index_for_mouse_position(event.position);
            self.selection.select_to(offset);
            cx.notify();
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return self.content.len();
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        previous_grapheme_boundary(&self.content, offset)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        next_grapheme_boundary(&self.content, offset)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        utf8_offset_from_utf16(&self.content, offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        utf16_offset_from_utf8(&self.content, offset)
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn replacement_range(&self, range: Option<&Range<usize>>) -> Range<usize> {
        range
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selection.range.clone())
    }

    fn replace_range(&mut self, range: Range<usize>, new_text: &str) -> usize {
        let cursor = range.start + new_text.len();
        let mut content =
            String::with_capacity(self.content.len() - (range.end - range.start) + new_text.len());
        content.push_str(&self.content[..range.start]);
        content.push_str(new_text);
        content.push_str(&self.content[range.end..]);
        self.content = content.into();
        cursor
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection.range),
            reversed: self.selection.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked_range.take().is_some() {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range.as_ref());
        let cursor = self.replace_range(range, new_text);
        self.selection.move_to(cursor);
        self.marked_range = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        selected_range: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.replacement_range(range.as_ref());
        let marked_start = range.start;
        let marked_end = self.replace_range(range, new_text);
        self.marked_range = (!new_text.is_empty()).then_some(marked_start..marked_end);
        if let Some(selected_range) = selected_range {
            let selected_range = utf8_range_from_utf16(new_text, &selected_range);
            self.selection.range =
                marked_start + selected_range.start..marked_start + selected_range.end;
            self.selection.reversed = false;
        } else {
            self.selection.move_to(marked_end);
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let line = self.last_layout.as_ref()?;
        Some(Bounds::from_corners(
            gpui::point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            gpui::point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.index_for_mouse_position(position)))
    }
}

fn previous_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(index, _)| (index < offset).then_some(index))
        .unwrap_or(0)
}

fn next_grapheme_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(index, _)| (index > offset).then_some(index))
        .unwrap_or(text.len())
}

fn utf8_offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_offset = 0;
    for character in text.chars() {
        if utf16_offset >= offset {
            break;
        }
        utf8_offset += character.len_utf8();
        utf16_offset += character.len_utf16();
    }
    utf8_offset
}

fn utf16_offset_from_utf8(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_offset = 0;
    for character in text.chars() {
        if utf8_offset >= offset {
            break;
        }
        utf8_offset += character.len_utf8();
        utf16_offset += character.len_utf16();
    }
    utf16_offset
}

fn utf8_range_from_utf16(text: &str, range: &Range<usize>) -> Range<usize> {
    utf8_offset_from_utf16(text, range.start)..utf8_offset_from_utf16(text, range.end)
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let input = cx.entity().downgrade();
        div()
            .id(("text-input", cx.entity_id()))
            .key_context("TextInput")
            .track_focus(&self.focus_handle.clone().tab_stop(true))
            .role(Role::TextInput)
            .aria_label(self.placeholder.clone())
            .aria_placeholder(self.placeholder.clone())
            .aria_value(self.content.clone())
            .on_a11y_action(AccessibleAction::SetValue, move |data, _, cx| {
                let Some(ActionData::Value(value)) = data else {
                    return;
                };
                let _ = input.update(cx, |input, cx| input.set_text(value.to_string(), cx));
            })
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .flex()
            .items_center()
            .w_full()
            .h(px(32.))
            .px_2()
            .rounded_sm()
            .bg(cx.theme().colors().element_background)
            .border_1()
            .border_color(if is_focused {
                cx.theme().colors().border_focused
            } else {
                cx.theme().colors().border
            })
            .child(InputElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_convert_across_surrogate_pairs() {
        let text = "A😀B";

        assert_eq!(
            (0..=4)
                .map(|offset| utf8_offset_from_utf16(text, offset))
                .collect::<Vec<_>>(),
            [0, 1, 5, 5, 6]
        );
    }

    #[test]
    fn utf8_offsets_convert_to_utf16_code_units() {
        let text = "A😀B";

        assert_eq!(
            [0, 1, 5, 6].map(|offset| utf16_offset_from_utf8(text, offset)),
            [0, 1, 3, 4]
        );
    }

    #[test]
    fn selecting_left_preserves_the_right_hand_anchor() {
        let mut selection = TextSelection::caret(3);

        selection.select_to(1);
        selection.select_to(2);

        assert_eq!(
            selection,
            TextSelection {
                range: 2..3,
                reversed: true,
            }
        );
    }

    #[test]
    fn crossing_the_selection_anchor_reverses_its_direction() {
        let mut selection = TextSelection::caret(3);

        selection.select_to(1);
        selection.select_to(4);

        assert_eq!(
            selection,
            TextSelection {
                range: 3..4,
                reversed: false,
            }
        );
    }

    #[test]
    fn extended_graphemes_have_single_movement_boundaries() {
        let text = "a\u{301}👩‍💻b";

        assert_eq!(
            (
                previous_grapheme_boundary(text, 14),
                previous_grapheme_boundary(text, 3),
                next_grapheme_boundary(text, 0),
                next_grapheme_boundary(text, 3),
            ),
            (3, 0, 3, 14)
        );
    }
}
