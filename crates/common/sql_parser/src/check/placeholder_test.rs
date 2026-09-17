//! Unit tests for the placeholder span API.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::check::placeholder::placeholder_spans;

fn texts(sql: &str) -> Vec<(String, usize)> {
    placeholder_spans(sql)
        .unwrap()
        .into_iter()
        .map(|(range, index)| (sql[range].to_string(), index))
        .collect()
}

#[test]
#[cfg_attr(miri, ignore)]
fn no_placeholders_returns_empty() {
    assert!(placeholder_spans("").unwrap().is_empty());
    assert!(placeholder_spans("SELECT 1").unwrap().is_empty());
    assert!(placeholder_spans("SELECT a FROM t").unwrap().is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn question_placeholders_are_ordered_and_indexed() {
    let spans = texts("SELECT * FROM t WHERE a = ? AND b = ?");
    assert_eq!(spans, vec![("?".to_string(), 0), ("?".to_string(), 1)]);

    let sql = "INSERT INTO t (a, b, c) VALUES (?, ?, ?)";
    let spans = placeholder_spans(sql).unwrap();
    assert_eq!(spans.len(), 3);
    // Byte ranges are in order of appearance and cover exactly one `?`.
    assert!(spans[0].0.start < spans[1].0.start);
    assert!(spans[1].0.start < spans[2].0.start);
    assert_eq!(
        spans
            .iter()
            .map(|(range, index)| (range.end - range.start, *index))
            .collect::<Vec<_>>(),
        vec![(1, 0), (1, 1), (1, 2)]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn dollar_number_placeholders_are_found() {
    let spans = texts("SELECT a FROM t WHERE x = $1 AND y = $2");
    assert_eq!(spans, vec![("$1".to_string(), 0), ("$2".to_string(), 1)]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn placeholders_in_string_literals_are_ignored() {
    assert!(texts("SELECT 'a?b'").is_empty());
    let spans = texts("SELECT 'a?b' FROM t WHERE x = ?");
    assert_eq!(spans, vec![("?".to_string(), 0)]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn placeholders_in_comments_are_ignored() {
    let spans = texts("-- a comment with ?\nSELECT a FROM t WHERE x = ?");
    assert_eq!(spans, vec![("?".to_string(), 0)]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn placeholders_in_update_and_delete_are_found() {
    let spans = texts("UPDATE t SET a = ?, b = ? WHERE c = ?");
    assert_eq!(spans.len(), 3);
    let spans = texts("DELETE FROM t WHERE c = ?");
    assert_eq!(spans.len(), 1);
}
