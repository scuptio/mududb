use super::utils::{
    find_keyword_position, find_matching_paren, split_top_level_csv, starts_with_ignore_ascii_case,
};
use crate::ast::stmt_create_partition_placement::StmtPartitionPlacementItem;
use crate::ast::stmt_create_partition_rule::{StmtPartitionBound, StmtRangePartition};
use crate::ast::stmt_table_partition::StmtTablePartition;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

pub(crate) fn parse_table_partition_suffix(input: &str) -> RS<StmtTablePartition> {
    let prefix = "partition by global rule ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "expected PARTITION BY GLOBAL RULE clause"
        ));
    }
    let rest = input[prefix.len()..].trim();
    let references_pos = find_keyword_position(rest, "references")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "partition clause must contain REFERENCES"))?;
    let rule_name = rest[..references_pos].trim();
    let refs = rest[references_pos + "references".len()..].trim();
    if !refs.starts_with('(') {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "REFERENCES clause must be wrapped in parentheses"
        ));
    }
    let close_index = find_matching_paren(refs, 0)?;
    let cols = split_top_level_csv(&refs[1..close_index])
        .into_iter()
        .map(|col| col.trim().to_string())
        .filter(|col| !col.is_empty())
        .collect::<Vec<_>>();
    if rule_name.is_empty() || cols.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid table partition clause"
        ));
    }
    Ok(StmtTablePartition::new(rule_name.to_string(), cols))
}

pub(crate) fn parse_range_partition_def(input: &str) -> RS<StmtRangePartition> {
    let prefix = "partition ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("invalid partition definition {}", input)
        ));
    }
    let rest = input[prefix.len()..].trim();
    let values_pos = find_keyword_position(rest, "values")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "partition definition must contain VALUES"))?;
    let name = rest[..values_pos].trim();
    let after_values = rest[values_pos + "values".len()..].trim();
    if !starts_with_ignore_ascii_case(after_values, "from") {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "partition definition must contain FROM"
        ));
    }
    let after_from = after_values["from".len()..].trim();
    let from_close = find_matching_paren(after_from, 0)?;
    let start = parse_partition_bound(&after_from[..=from_close])?;
    let after_start = after_from[from_close + 1..].trim();
    if !starts_with_ignore_ascii_case(after_start, "to") {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "partition definition must contain TO"
        ));
    }
    let after_to = after_start["to".len()..].trim();
    let end_close = find_matching_paren(after_to, 0)?;
    let end = parse_partition_bound(&after_to[..=end_close])?;
    Ok(StmtRangePartition::new(name.to_string(), start, end))
}

pub(crate) fn parse_partition_bound(input: &str) -> RS<StmtPartitionBound> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "partition bound must be parenthesized"
        ));
    }
    let items = split_top_level_csv(&trimmed[1..trimmed.len() - 1]);
    if items.len() == 1
        && (items[0].eq_ignore_ascii_case("minvalue") || items[0].eq_ignore_ascii_case("maxvalue"))
    {
        return Ok(StmtPartitionBound::Unbounded);
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        if item.eq_ignore_ascii_case("minvalue") || item.eq_ignore_ascii_case("maxvalue") {
            return Ok(StmtPartitionBound::Unbounded);
        }
        values.push(item.trim().as_bytes().to_vec());
    }
    Ok(StmtPartitionBound::Value(values))
}

pub(crate) fn parse_partition_placement_item(input: &str) -> RS<StmtPartitionPlacementItem> {
    let prefix = "partition ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("invalid partition placement item {}", input)
        ));
    }
    let rest = input[prefix.len()..].trim();
    let on_worker = find_keyword_position(rest, "on worker").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "partition placement item must contain ON WORKER"
        )
    })?;
    let partition_name = rest[..on_worker].trim();
    let worker_id = rest[on_worker + "on worker".len()..].trim();
    if partition_name.is_empty() || worker_id.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("invalid partition placement item {}", input)
        ));
    }
    Ok(StmtPartitionPlacementItem::new(
        partition_name.to_string(),
        worker_id.to_string(),
    ))
}
