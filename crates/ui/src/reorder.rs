/// Moves the item at `source_index` to `target_index`, shifting everything
/// between them by one. No-op if the indices are equal or out of bounds.
pub fn move_item<T>(items: &mut Vec<T>, source_index: usize, target_index: usize) {
    if source_index == target_index || source_index >= items.len() || target_index >= items.len() {
        return;
    }
    let item = items.remove(source_index);
    items.insert(target_index, item);
}

#[cfg(test)]
mod tests {
    use super::move_item;

    #[test]
    fn moves_items_in_both_directions() {
        let mut items = vec![1, 2, 3];

        move_item(&mut items, 0, 2);
        assert_eq!(items, [2, 3, 1]);

        move_item(&mut items, 2, 0);
        assert_eq!(items, [1, 2, 3]);
    }

    #[test]
    fn ignores_out_of_bounds_or_equal_indices() {
        let mut items = vec![1, 2, 3];

        move_item(&mut items, 1, 1);
        assert_eq!(items, [1, 2, 3]);

        move_item(&mut items, 0, 5);
        assert_eq!(items, [1, 2, 3]);
    }
}
