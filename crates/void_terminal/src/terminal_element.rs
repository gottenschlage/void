//! GPUI painting and platform text input for a terminal session.

use std::collections::{HashMap, hash_map::Entry};

use gpui::{
    App, Bounds, Entity, Font, FontStyle, FontWeight, Hsla, InputHandler, IntoElement, Pixels,
    Point as GpuiPoint, ShapedLine, StrikethroughStyle, TextAlign, TextRun, UTF16Selection,
    UnderlineStyle, Window, canvas, fill, font, point, prelude::*, px, rgb, size,
};
use terminal::{
    Cell, Color, Content, CursorShape, IndexedCell, Modes, NamedColor, Point, SelectionRange,
    Terminal, TerminalBounds,
};

use crate::{
    TerminalSettings,
    session::{SessionState, TerminalSession},
};

struct PaintState {
    backgrounds: Vec<(Bounds<Pixels>, Hsla)>,
    lines: Vec<(GpuiPoint<Pixels>, ShapedLine)>,
    cursor: Option<(Bounds<Pixels>, CursorShape, Hsla)>,
    cursor_text: Option<(GpuiPoint<Pixels>, ShapedLine)>,
    ime_bounds: Bounds<Pixels>,
    line_height: Pixels,
}

#[derive(Clone, PartialEq)]
struct CellStyle {
    font: Font,
    color: Hsla,
    underline: Option<UnderlineStyle>,
    strikethrough: Option<StrikethroughStyle>,
}

struct TextBatch {
    row: usize,
    column: usize,
    next_column: usize,
    text: String,
    style: CellStyle,
}

struct PaintParams<'a> {
    settings: &'a TerminalSettings,
    base_font: Font,
    font_size: Pixels,
    focused: bool,
    cursor_visible: bool,
    dimensions: TerminalBounds,
}

pub(crate) fn terminal_element(
    terminal: Entity<Terminal>,
    session: Entity<TerminalSession>,
    settings: TerminalSettings,
    focused: bool,
    cursor_visible: bool,
) -> impl IntoElement {
    let prepaint_session = session.clone();
    canvas(
        move |bounds, window, cx| {
            let font_size = px(settings.font_size);
            let base_font = font(settings.font_family.clone());
            let measure = window.text_system().shape_line(
                "m".into(),
                font_size,
                &[TextRun {
                    len: 1,
                    font: base_font.clone(),
                    color: rgb(settings.foreground).into(),
                    ..Default::default()
                }],
                None,
            );
            let cell_width = measure.width().ceil().max(px(1.));
            let line_height = px(settings.font_size * settings.line_height).max(px(1.));
            let scale = window.scale_factor().max(1.);
            let snap = |value: Pixels| px((f32::from(value) * scale).floor() / scale);
            let terminal_bounds = TerminalBounds::new(
                line_height,
                cell_width,
                Bounds {
                    origin: point(snap(bounds.origin.x), snap(bounds.origin.y)),
                    size: bounds.size,
                },
            );

            terminal.update(cx, |terminal, cx| {
                terminal.set_size(terminal_bounds);
                terminal.sync(window, cx);
            });
            let marked_text = prepaint_session.read(cx).marked_text.clone();
            build_paint_state(
                terminal.read(cx).last_content(),
                marked_text.as_deref(),
                PaintParams {
                    settings: &settings,
                    base_font,
                    font_size,
                    focused,
                    cursor_visible,
                    dimensions: terminal_bounds,
                },
                window,
            )
        },
        move |_, state, window, cx| {
            for (bounds, color) in state.backgrounds {
                window.paint_quad(fill(bounds, color));
            }
            for (origin, line) in state.lines {
                let _ = line.paint(origin, state.line_height, TextAlign::Left, None, window, cx);
            }
            if let Some((bounds, shape, color)) = state.cursor {
                paint_cursor(bounds, shape, color, window);
            }
            if let Some((origin, line)) = state.cursor_text {
                let _ = line.paint(origin, state.line_height, TextAlign::Left, None, window, cx);
            }
            window.handle_input(
                &session.read(cx).focus,
                TerminalInputHandler {
                    session,
                    cursor_bounds: state.ime_bounds,
                },
                cx,
            );
        },
    )
    .size_full()
}

fn build_paint_state(
    content: &Content,
    marked_text: Option<&str>,
    params: PaintParams<'_>,
    window: &mut Window,
) -> PaintState {
    let PaintParams {
        settings,
        base_font,
        font_size,
        focused,
        cursor_visible,
        dimensions,
    } = params;
    let rows = display_rows(&content.cells);
    let mut backgrounds = Vec::new();
    let mut batches = Vec::<TextBatch>::new();
    let mut current: Option<TextBatch> = None;

    for indexed in &content.cells {
        let row = rows[&indexed.point.line];
        let selected = is_selected(content.selection, indexed.point);
        let (foreground, background) = cell_colors(&indexed.cell, settings);
        let background = if selected {
            Some(rgb(settings.selection_background).into())
        } else {
            background
        };
        if let Some(background) = background {
            backgrounds.push((
                cell_bounds(dimensions, row, indexed.point.column, 1),
                background,
            ));
        }

        if indexed.cell.is_wide_char_spacer() {
            if let Some(batch) = current.as_mut()
                && batch.row == row
                && batch.next_column == indexed.point.column
            {
                batch.next_column += 1;
            }
            continue;
        }

        let color = if selected {
            rgb(settings.selection_foreground).into()
        } else if indexed.cell.is_dim() {
            foreground.opacity(0.66)
        } else {
            foreground
        };
        let style = cell_style(&indexed.cell, base_font.clone(), color);
        let mut text = indexed.cell.character().to_string();
        if let Some(chars) = indexed.cell.zerowidth() {
            text.extend(chars);
        }

        let can_append = current.as_ref().is_some_and(|batch| {
            batch.row == row && batch.next_column == indexed.point.column && batch.style == style
        });
        if can_append && let Some(batch) = current.as_mut() {
            batch.text.push_str(&text);
            batch.next_column += 1;
        } else {
            if let Some(batch) = current.take() {
                batches.push(batch);
            }
            current = Some(TextBatch {
                row,
                column: indexed.point.column,
                next_column: indexed.point.column + 1,
                text,
                style,
            });
        }
    }
    if let Some(batch) = current {
        batches.push(batch);
    }

    let mut lines: Vec<_> = batches
        .into_iter()
        .map(|batch| {
            let run = TextRun {
                len: batch.text.len(),
                font: batch.style.font,
                color: batch.style.color,
                background_color: None,
                underline: batch.style.underline,
                strikethrough: batch.style.strikethrough,
            };
            let line = window
                .text_system()
                .shape_line(batch.text.into(), font_size, &[run], None);
            (
                point(
                    dimensions.bounds.origin.x + dimensions.cell_width * batch.column as f32,
                    dimensions.bounds.origin.y + dimensions.line_height * batch.row as f32,
                ),
                line,
            )
        })
        .collect();

    let cursor_row = content.cursor.point.line + content.display_offset as i32;
    let cursor_origin = point(
        dimensions.bounds.origin.x + dimensions.cell_width * content.cursor.point.column as f32,
        dimensions.bounds.origin.y + dimensions.line_height * cursor_row as f32,
    );
    let ime_bounds = Bounds::new(
        cursor_origin,
        size(dimensions.cell_width, dimensions.line_height),
    );
    let cursor = (content.mode.contains(Modes::SHOW_CURSOR)
        && content.cursor.shape != CursorShape::Hidden
        && (!focused || cursor_visible))
        .then(|| {
            (
                ime_bounds,
                if focused {
                    content.cursor.shape
                } else {
                    CursorShape::HollowBlock
                },
                rgb(settings.cursor_color).into(),
            )
        });
    if let Some(marked_text) = marked_text.filter(|text| !text.is_empty()) {
        let line = window.text_system().shape_line(
            marked_text.to_owned().into(),
            font_size,
            &[TextRun {
                len: marked_text.len(),
                font: base_font.clone(),
                color: rgb(settings.foreground).into(),
                underline: Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(rgb(settings.foreground).into()),
                    wavy: false,
                }),
                ..Default::default()
            }],
            None,
        );
        lines.push((cursor_origin, line));
    }
    let cursor_text = (cursor.is_some()
        && focused
        && content.cursor.shape == CursorShape::Block
        && !content.cursor_char.is_whitespace())
    .then(|| {
        let text = content.cursor_char.to_string();
        let line = window.text_system().shape_line(
            text.clone().into(),
            font_size,
            &[TextRun {
                len: text.len(),
                font: base_font,
                color: settings
                    .background
                    .map_or_else(|| rgb(0x000000).into(), |color| rgb(color).into()),
                ..Default::default()
            }],
            None,
        );
        (cursor_origin, line)
    });

    PaintState {
        backgrounds,
        lines,
        cursor,
        cursor_text,
        ime_bounds,
        line_height: dimensions.line_height,
    }
}

fn display_rows(cells: &[IndexedCell]) -> HashMap<i32, usize> {
    let mut rows = HashMap::new();
    let mut next_row = 0;
    for cell in cells {
        if let Entry::Vacant(entry) = rows.entry(cell.point.line) {
            entry.insert(next_row);
            next_row += 1;
        }
    }
    rows
}

fn is_selected(selection: Option<SelectionRange>, point: Point) -> bool {
    selection.is_some_and(|selection| {
        if selection.is_block {
            let start_line = selection.start.line.min(selection.end.line);
            let end_line = selection.start.line.max(selection.end.line);
            let start_column = selection.start.column.min(selection.end.column);
            let end_column = selection.start.column.max(selection.end.column);
            (start_line..=end_line).contains(&point.line)
                && (start_column..=end_column).contains(&point.column)
        } else {
            selection.point_range().contains(point)
        }
    })
}

fn cell_bounds(
    dimensions: TerminalBounds,
    row: usize,
    column: usize,
    width: usize,
) -> Bounds<Pixels> {
    Bounds::new(
        point(
            dimensions.bounds.origin.x + dimensions.cell_width * column as f32,
            dimensions.bounds.origin.y + dimensions.line_height * row as f32,
        ),
        size(dimensions.cell_width * width as f32, dimensions.line_height),
    )
}

fn cell_style(cell: &Cell, mut font: Font, color: Hsla) -> CellStyle {
    if cell.is_bold() {
        font.weight = FontWeight::BOLD;
    }
    if cell.is_italic() {
        font.style = FontStyle::Italic;
    }
    let underline =
        (cell.has_underline() || cell.hyperlink().is_some()).then_some(UnderlineStyle {
            thickness: px(1.),
            color: Some(color),
            wavy: cell.has_undercurl(),
        });
    let strikethrough = cell.has_strikeout().then_some(StrikethroughStyle {
        thickness: px(1.),
        color: Some(color),
    });
    CellStyle {
        font,
        color,
        underline,
        strikethrough,
    }
}

fn cell_colors(cell: &Cell, settings: &TerminalSettings) -> (Hsla, Option<Hsla>) {
    let mut foreground = resolve_color(cell.foreground(), settings, false);
    let mut background = resolve_color(cell.background(), settings, true);
    if cell.is_inverse() {
        std::mem::swap(&mut foreground, &mut background);
    }
    (
        foreground.unwrap_or_else(|| rgb(settings.foreground).into()),
        background,
    )
}

fn resolve_color(color: Color, settings: &TerminalSettings, background: bool) -> Option<Hsla> {
    let value = match color {
        Color::Spec(color) => {
            return Some(
                rgb((u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b))
                    .into(),
            );
        }
        Color::Indexed(index) => indexed_color(index, settings),
        Color::Named(NamedColor::Foreground | NamedColor::BrightForeground) => settings.foreground,
        Color::Named(NamedColor::Background) => {
            return settings.background.map(|color| rgb(color).into());
        }
        Color::Named(NamedColor::Cursor) => settings.cursor_color,
        Color::Named(named) if (named as usize) < settings.ansi.len() => {
            settings.ansi[named as usize]
        }
        Color::Named(named) => dim_named_color(named, settings),
    };
    if background && matches!(color, Color::Named(NamedColor::Background)) {
        settings.background.map(|color| rgb(color).into())
    } else {
        Some(rgb(value).into())
    }
}

fn indexed_color(index: u8, settings: &TerminalSettings) -> u32 {
    match index {
        0..=15 => settings.ansi[index as usize],
        16..=231 => {
            let index = u32::from(index - 16);
            let component = |value: u32| if value == 0 { 0 } else { value * 40 + 55 };
            let red = component(index / 36);
            let green = component((index / 6) % 6);
            let blue = component(index % 6);
            (red << 16) | (green << 8) | blue
        }
        232..=255 => {
            let value = 8 + u32::from(index - 232) * 10;
            (value << 16) | (value << 8) | value
        }
    }
}

fn dim_named_color(named: NamedColor, settings: &TerminalSettings) -> u32 {
    let index = match named {
        NamedColor::DimBlack => 0,
        NamedColor::DimRed => 1,
        NamedColor::DimGreen => 2,
        NamedColor::DimYellow => 3,
        NamedColor::DimBlue => 4,
        NamedColor::DimMagenta => 5,
        NamedColor::DimCyan => 6,
        NamedColor::DimWhite | NamedColor::DimForeground => 7,
        _ => 7,
    };
    dim_rgb(settings.ansi[index])
}

fn dim_rgb(color: u32) -> u32 {
    let dim = |component: u32| (component * 2) / 3;
    (dim((color >> 16) & 0xff) << 16) | (dim((color >> 8) & 0xff) << 8) | dim(color & 0xff)
}

fn paint_cursor(bounds: Bounds<Pixels>, shape: CursorShape, color: Hsla, window: &mut Window) {
    let bounds = match shape {
        CursorShape::Underline => Bounds::new(
            point(bounds.left(), bounds.bottom() - px(2.)),
            size(bounds.size.width, px(2.)),
        ),
        CursorShape::Bar => Bounds::new(bounds.origin, size(px(2.), bounds.size.height)),
        CursorShape::Block => bounds,
        CursorShape::HollowBlock => {
            window.paint_quad(fill(
                Bounds::new(bounds.origin, size(bounds.size.width, px(1.))),
                color,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.left(), bounds.bottom() - px(1.)),
                    size(bounds.size.width, px(1.)),
                ),
                color,
            ));
            window.paint_quad(fill(
                Bounds::new(bounds.origin, size(px(1.), bounds.size.height)),
                color,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.right() - px(1.), bounds.top()),
                    size(px(1.), bounds.size.height),
                ),
                color,
            ));
            return;
        }
        CursorShape::Hidden => return,
    };
    window.paint_quad(fill(bounds, color));
}

struct TerminalInputHandler {
    session: Entity<TerminalSession>,
    cursor_bounds: Bounds<Pixels>,
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &mut self,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<std::ops::Range<usize>> {
        self.session
            .read(cx)
            .marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn text_for_range(
        &mut self,
        _: std::ops::Range<usize>,
        _: &mut Option<std::ops::Range<usize>>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<std::ops::Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut App,
    ) {
        self.session.update(cx, |session, cx| {
            session.marked_text = None;
            if let SessionState::Ready { terminal, .. } = &session.state
                && !text.is_empty()
            {
                terminal.update(cx, |terminal, _| terminal.input(text.as_bytes().to_vec()));
            }
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<std::ops::Range<usize>>,
        text: &str,
        _: Option<std::ops::Range<usize>>,
        _: &mut Window,
        cx: &mut App,
    ) {
        self.session.update(cx, |session, cx| {
            session.marked_text = (!text.is_empty()).then(|| text.to_owned());
            cx.notify();
        });
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut App) {
        self.session.update(cx, |session, cx| {
            session.marked_text = None;
            cx.notify();
        });
    }

    fn bounds_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let mut bounds = self.cursor_bounds;
        bounds.origin.x += bounds.size.width * range.start as f32;
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: GpuiPoint<Pixels>,
        _: &mut Window,
        _: &mut App,
    ) -> Option<usize> {
        None
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{dim_rgb, indexed_color};
    use crate::TerminalSettings;

    #[test]
    fn resolves_256_color_palette() {
        let settings = TerminalSettings::default();
        assert_eq!(indexed_color(0, &settings), settings.ansi[0]);
        assert_eq!(indexed_color(16, &settings), 0x000000);
        assert_eq!(indexed_color(21, &settings), 0x0000ff);
        assert_eq!(indexed_color(231, &settings), 0xffffff);
        assert_eq!(indexed_color(232, &settings), 0x080808);
        assert_eq!(indexed_color(255, &settings), 0xeeeeee);
    }

    #[test]
    fn dims_each_rgb_component() {
        assert_eq!(dim_rgb(0xff8040), 0xaa552a);
    }
}
