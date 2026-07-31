//! Pure terminal-tab ordering and selection state.

use ui::move_item;

/// Stable identity for one terminal session within a branch panel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TerminalId(pub(super) u64);

#[derive(Default)]
pub(super) struct TerminalTabs {
    pub(super) order: Vec<TerminalId>,
    pub(super) active: Option<TerminalId>,
    next_id: u64,
}

impl TerminalTabs {
    pub(super) fn insert_new(&mut self) -> TerminalId {
        self.next_id += 1;
        let id = TerminalId(self.next_id);
        self.order.insert(0, id);
        self.active = Some(id);
        id
    }

    pub(super) fn select(&mut self, id: TerminalId) {
        if self.order.contains(&id) {
            self.active = Some(id);
        }
    }

    pub(super) fn close(&mut self, id: TerminalId) -> Option<bool> {
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
    pub(super) fn reorder(&mut self, id: TerminalId, target: usize) {
        let Some(source) = self.order.iter().position(|candidate| *candidate == id) else {
            return;
        };
        if source == target {
            return;
        }
        move_item(&mut self.order, source, target);
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalTabs;

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
    fn closing_middle_active_tab_selects_its_right_neighbor() {
        let mut tabs = TerminalTabs::default();
        let first = tabs.insert_new();
        let second = tabs.insert_new();
        let _third = tabs.insert_new();
        tabs.select(second);

        assert_eq!(tabs.close(second), Some(true));
        assert_eq!(tabs.active, Some(first));
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
