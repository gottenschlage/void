use gpui::{InteractiveElement, Styled};

use crate::scaled;

/// The list-row shape shared by sidebar rows, menu items, and popover
/// entries: fixed height, horizontal padding, rounded corners. Callers apply
/// their own background, text color, and `cursor_pointer()` if the row is
/// clickable (some rows, like a repository's outer row, only have clickable
/// children).
pub trait ListRow: Styled + InteractiveElement + Sized {
    fn list_row(self, height: f32) -> Self {
        self.flex()
            .h(scaled(height))
            .items_center()
            .px_2()
            .rounded_sm()
            .text_sm()
    }
}

impl<E: Styled + InteractiveElement> ListRow for E {}
