//! Paint order of the objects inside their own kind's array (`web/src/sheet-object/order.ts`). Order is per
//! kind on the web too: « bring to front » can never lift a picture above a chart.

/// `ObjOrderOp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderOp {
    Front,
    Forward,
    Backward,
    Back,
}

/// `orderTarget`.
pub fn order_target(len: usize, index: usize, op: OrderOp) -> usize {
    match op {
        OrderOp::Front => len.saturating_sub(1),
        OrderOp::Back => 0,
        OrderOp::Forward => (index + 1).min(len.saturating_sub(1)),
        OrderOp::Backward => index.saturating_sub(1),
    }
}

/// `canOrder`.
pub fn can_order(len: usize, index: usize, op: OrderOp) -> bool {
    index < len && len >= 2 && order_target(len, index, op) != index
}

/// `reorderAt`: the new index, `None` when nothing moved.
pub fn reorder_at<T>(items: &mut Vec<T>, index: usize, op: OrderOp) -> Option<usize> {
    if index >= items.len() {
        return None;
    }
    let to = order_target(items.len(), index, op);
    if to == index {
        return None;
    }
    let moved = items.remove(index);
    items.insert(to, moved);
    Some(to)
}

/// `reorderMany`: a multi-selection moved as a block, keeping its relative order.
pub fn reorder_many<T: Clone>(items: &[T], indices: &[usize], op: OrderOp) -> Vec<T> {
    let mut sel: Vec<usize> = indices.iter().copied().filter(|i| *i < items.len()).collect();
    sel.sort_unstable();
    sel.dedup();
    if sel.is_empty() || sel.len() == items.len() {
        return items.to_vec();
    }
    if matches!(op, OrderOp::Front | OrderOp::Back) {
        let block: Vec<T> = sel.iter().map(|i| items[*i].clone()).collect();
        let rest: Vec<T> = items.iter().enumerate().filter(|(i, _)| !sel.contains(i)).map(|(_, x)| x.clone()).collect();
        return if op == OrderOp::Front { rest.into_iter().chain(block).collect() } else { block.into_iter().chain(rest).collect() };
    }
    let mut next = items.to_vec();
    let mut in_sel: std::collections::HashSet<usize> = sel.iter().copied().collect();
    if op == OrderOp::Forward {
        for k in (0..sel.len()).rev() {
            let i = sel[k];
            if i + 1 >= next.len() || in_sel.contains(&(i + 1)) {
                continue;
            }
            next.swap(i, i + 1);
            in_sel.remove(&i);
            in_sel.insert(i + 1);
            sel[k] = i + 1;
        }
    } else {
        for slot in sel.iter_mut() {
            let i = *slot;
            if i == 0 || in_sel.contains(&(i - 1)) {
                continue;
            }
            next.swap(i, i - 1);
            in_sel.remove(&i);
            in_sel.insert(i - 1);
            *slot = i - 1;
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_keep_their_order() {
        let v = vec!['a', 'b', 'c', 'd'];
        assert_eq!(reorder_many(&v, &[0, 1], OrderOp::Front), vec!['c', 'd', 'a', 'b']);
        assert_eq!(reorder_many(&v, &[0, 2], OrderOp::Forward), vec!['b', 'a', 'd', 'c']);
        assert_eq!(reorder_many(&v, &[1, 3], OrderOp::Backward), vec!['b', 'a', 'd', 'c']);
        let mut w = v.clone();
        assert_eq!(reorder_at(&mut w, 0, OrderOp::Front), Some(3));
        assert_eq!(w, vec!['b', 'c', 'd', 'a']);
        assert!(!can_order(4, 3, OrderOp::Forward));
    }
}
