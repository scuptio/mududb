//! Exact split sizing without constructing a pair of nodes for every candidate.
use super::{
    codec::HEADER,
    invalid,
    node::{Key, Node},
    RS,
};
use std::sync::Arc;

fn common(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(a, b)| a == b).count()
}

/// Prefix/suffix summaries make each candidate size and balance check constant
/// time. Preserve the previous minimum-weight-difference policy and its tie order.
pub(super) fn divide(node: &Node, size: usize) -> RS<(Node, Key, Node)> {
    let n = node.entries.len();
    let mut prefix = vec![0; n + 1];
    let mut suffix = vec![0; n + 1];
    let mut heads = vec![0; n + 1];
    for i in 0..n {
        let head = &node.entries[i].key.head;
        heads[i + 1] = heads[i] + head.len();
        prefix[i + 1] = if i == 0 {
            head.len()
        } else {
            common(&node.entries[0].key.head[..prefix[i]], head)
        };
    }
    for i in (0..n).rev() {
        suffix[i] = if i + 1 == n {
            node.entries[i].key.head.len()
        } else {
            common(
                &node.entries[i + 1].key.head[..suffix[i + 1]],
                &node.entries[i].key.head,
            )
        };
    }
    let length = |start: usize, end: usize, pre: usize, high: usize| {
        let count = end - start;
        HEADER + pre + (count + 1) * 4 + high + count * 32 + heads[end] - heads[start] - count * pre
    };
    let mut best = None;
    let mut balance = usize::MAX;
    for mid in 1..n {
        let right = mid + usize::from(!node.leaf());
        let high = 16 + node.entries[mid].key.head.len();
        if length(0, mid, prefix[mid], high) > size
            || length(
                right,
                n,
                suffix[right],
                node.high.as_ref().map_or(0, |key| 16 + key.head.len()),
            ) > size
        {
            continue;
        }
        let delta = (heads[mid] + mid * 28).abs_diff(heads[n] - heads[right] + (n - right) * 28);
        if delta < balance {
            best = Some(mid);
            balance = delta;
        }
    }
    let mid = best.ok_or_else(|| invalid("cannot split index node"))?;
    let separator = node.entries[mid].key.clone();
    let mut left = node.clone();
    Arc::make_mut(&mut left.entries).truncate(mid);
    left.high = Some(separator.clone());
    let mut right = Node::empty(node.level);
    right.high = node.high.clone();
    right.right = node.right.clone();
    let start = if node.leaf() {
        mid
    } else {
        right.left = node.entries[mid].target.clone();
        mid + 1
    };
    right.entries = node.entries[start..].to_vec().into();
    Ok((left, separator, right))
}
