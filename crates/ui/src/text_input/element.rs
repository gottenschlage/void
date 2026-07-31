//! Low-level layout, shaping, and painting for [`TextInput`](super::TextInput).

use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, IntoElement,
    LayoutId, PaintQuad, Pixels, Style, TextRun, UnderlineStyle, Window, fill, point, relative,
    size,
};
use theme::ActiveTheme;

use super::TextInput;

pub(super) struct InputElement {
    pub(super) input: Entity<TextInput>,
}

pub(super) struct PrepaintState {
    line: Option<gpui::ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for InputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for InputElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let (text, color) = if input.content.is_empty() {
            (
                input.placeholder.clone(),
                cx.theme().colors().text_placeholder,
            )
        } else {
            (input.content.clone(), window.text_style().color)
        };
        let run = TextRun {
            len: text.len(),
            font: window.text_style().font(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            [
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: gpui::px(1.),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect::<Vec<_>>()
        } else {
            vec![run]
        };
        let line = window.text_system().shape_line(
            text,
            window.text_style().font_size.to_pixels(window.rem_size()),
            &runs,
            None,
        );
        let focused = input.focus_handle.is_focused(window);
        let cursor = (focused && input.selection.range.is_empty()).then(|| {
            fill(
                Bounds::new(
                    point(
                        bounds.left() + line.x_for_index(input.selection.cursor()),
                        bounds.top(),
                    ),
                    size(gpui::px(1.), bounds.size.height),
                ),
                window.text_style().color,
            )
        });
        let selection = (focused && !input.selection.range.is_empty()).then(|| {
            fill(
                Bounds::from_corners(
                    point(
                        bounds.left() + line.x_for_index(input.selection.range.start),
                        bounds.top(),
                    ),
                    point(
                        bounds.left() + line.x_for_index(input.selection.range.end),
                        bounds.bottom(),
                    ),
                ),
                cx.theme().colors().element_selection_background,
            )
        });
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = state.selection.take() {
            window.paint_quad(selection);
        }
        let Some(line) = state.line.take() else {
            return;
        };
        let _ = line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
        if let Some(cursor) = state.cursor.take() {
            window.paint_quad(cursor);
        }
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}
