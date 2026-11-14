//! Custom parsers for the queue statements (`CREATE QUEUE`, `DROP QUEUE`,
//! `RECEIVE`, `ACK`) and the consumer cursor statements (`CREATE CONSUMER`,
//! `DROP CONSUMER`) that the tree-sitter grammar does not cover.

use super::entry::{parse_u64_token, validate_type_name};
use super::utils::{find_matching_paren, starts_with_ignore_ascii_case};
use super::SQLParser;
use crate::ast::stmt_ack::StmtAck;
use crate::ast::stmt_create_consumer::StmtCreateConsumer;
use crate::ast::stmt_create_queue::{QueuePayload, StmtCreateQueue};
use crate::ast::stmt_drop_consumer::StmtDropConsumer;
use crate::ast::stmt_drop_queue::StmtDropQueue;
use crate::ast::stmt_list::StmtList;
use crate::ast::stmt_receive::{ReceiveWait, StmtReceive};
use crate::ast::stmt_table_partition::StmtTablePartition;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

impl SQLParser {
    /// Try to parse one queue-family statement (queue DDL, consumer cursor
    /// DDL, `RECEIVE`, `ACK`); `None` when the statement belongs to another
    /// family.
    pub(crate) fn try_parse_queue_statement(&self, normalized: &str) -> RS<Option<StmtList>> {
        if starts_with_ignore_ascii_case(normalized, "create queue ") {
            let stmt = self.parse_create_queue_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreateQueue(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "create consumer ")
            || normalized.eq_ignore_ascii_case("create consumer")
        {
            let stmt = self.parse_create_consumer_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreateConsumer(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "drop consumer ")
            || normalized.eq_ignore_ascii_case("drop consumer")
        {
            let stmt = self.parse_drop_consumer_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::DropConsumer(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "drop queue ")
            || normalized.eq_ignore_ascii_case("drop queue")
        {
            let stmt = self.parse_drop_queue_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::DropQueue(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "receive ")
            || normalized.eq_ignore_ascii_case("receive")
        {
            let stmt = self.parse_receive_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::Receive(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "ack ")
            || normalized.eq_ignore_ascii_case("ack")
        {
            let stmt = self.parse_ack_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::Ack(stmt),
            )])));
        }

        Ok(None)
    }

    /// Parse a `CREATE QUEUE <name> LIKE <table_name>` or
    /// `CREATE QUEUE <name> (<col> <type>, ...)` statement, each optionally
    /// followed by `PARTITION BY GLOBAL RULE <rule> REFERENCES (<col>)` to
    /// shard the queue by a payload column.
    pub(crate) fn parse_create_queue_custom(&self, sql: &str) -> RS<StmtCreateQueue> {
        let prefix = "create queue ";
        let rest = sql[prefix.len()..].trim();
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '(')
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Parse,
                    "create queue must specify LIKE or a column list"
                )
            })?;
        let name = &rest[..name_end];
        validate_type_name(name)?;
        let body = rest[name_end..].trim();
        if body.starts_with('(') {
            let close_index = find_matching_paren(body, 0)?;
            // Reuse the standard CREATE TABLE parser for the column list so
            // payload columns share the full type syntax.
            let create_table_sql = format!("create table {} {}", name, &body[..=close_index]);
            let stmt_list = self.parse_standard(&create_table_sql)?;
            let tail = body[close_index + 1..].trim();
            match stmt_list.stmts().first() {
                Some(StmtType::Command(StmtCommand::CreateTable(table))) => {
                    let mut stmt = StmtCreateQueue::new(
                        name.to_string(),
                        QueuePayload::Columns(table.column_def().clone()),
                    );
                    if let Some(partition) = parse_queue_partition_tail(tail)? {
                        stmt.set_partition(partition);
                    }
                    Ok(stmt)
                }
                _ => Err(mudu_error!(
                    ErrorCode::Parse,
                    "failed to parse create queue column list"
                )),
            }
        } else if starts_with_ignore_ascii_case(body, "like")
            && body["like".len()..].starts_with(char::is_whitespace)
        {
            let like_body = body["like".len()..].trim();
            let table_end = like_body
                .find(char::is_whitespace)
                .unwrap_or(like_body.len());
            let table_name = &like_body[..table_end];
            validate_type_name(table_name)?;
            let mut stmt =
                StmtCreateQueue::new(name.to_string(), QueuePayload::Like(table_name.to_string()));
            if let Some(partition) = parse_queue_partition_tail(like_body[table_end..].trim())? {
                stmt.set_partition(partition);
            }
            Ok(stmt)
        } else {
            Err(mudu_error!(
                ErrorCode::Parse,
                "create queue must specify LIKE or a column list"
            ))
        }
    }

    /// Parse a `DROP QUEUE <name>` statement.
    pub(crate) fn parse_drop_queue_custom(&self, sql: &str) -> RS<StmtDropQueue> {
        let prefix = "drop queue";
        let name = sql[prefix.len()..].trim();
        validate_type_name(name)?;
        Ok(StmtDropQueue::new(name.to_string()))
    }

    /// Parse a `RECEIVE [n] FROM <name> [FOR <consumer>] [WAIT <ms> | WAIT
    /// FOREVER]` statement.
    pub(crate) fn parse_receive_custom(&self, sql: &str) -> RS<StmtReceive> {
        let prefix = "receive";
        let rest = sql[prefix.len()..].trim();
        let mut tokens = rest.split_whitespace();
        let first = tokens
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "receive must specify FROM <name>"))?;
        let (count, name) = if first.eq_ignore_ascii_case("from") {
            let name = tokens.next().ok_or_else(|| {
                mudu_error!(ErrorCode::Parse, "receive must specify a queue name")
            })?;
            (None, name)
        } else {
            let count = parse_u64_token(first, "receive count")?;
            let from = tokens.next().ok_or_else(|| {
                mudu_error!(ErrorCode::Parse, "receive count must be followed by FROM")
            })?;
            if !from.eq_ignore_ascii_case("from") {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "receive count must be followed by FROM"
                ));
            }
            let name = tokens.next().ok_or_else(|| {
                mudu_error!(ErrorCode::Parse, "receive must specify a queue name")
            })?;
            (Some(count), name)
        };
        validate_type_name(name)?;
        let mut consumer: Option<String> = None;
        let mut keyword = tokens.next();
        if let Some(token) = keyword {
            if token.eq_ignore_ascii_case("for") {
                let name = tokens.next().ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::Parse,
                        "receive FOR requires a consumer cursor name"
                    )
                })?;
                validate_type_name(name)?;
                consumer = Some(name.to_string());
                keyword = tokens.next();
            }
        }
        let wait = match keyword {
            None => None,
            Some(keyword) => {
                if !keyword.eq_ignore_ascii_case("wait") {
                    return Err(mudu_error!(
                        ErrorCode::Parse,
                        format!("unexpected token {keyword} in receive statement")
                    ));
                }
                let value = tokens.next().ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::Parse,
                        "receive WAIT requires a millisecond count or FOREVER"
                    )
                })?;
                let wait = if value.eq_ignore_ascii_case("forever") {
                    ReceiveWait::Forever
                } else {
                    ReceiveWait::Timeout(parse_u64_token(value, "receive WAIT milliseconds")?)
                };
                if let Some(extra) = tokens.next() {
                    return Err(mudu_error!(
                        ErrorCode::Parse,
                        format!("unexpected trailing token {extra} in receive statement")
                    ));
                }
                Some(wait)
            }
        };
        let mut stmt = StmtReceive::new(name.to_string(), count, wait);
        if let Some(consumer) = consumer {
            stmt.set_consumer(consumer);
        }
        Ok(stmt)
    }

    /// Parse an `ACK <name> [FOR <consumer>] TO <seq>` statement.
    pub(crate) fn parse_ack_custom(&self, sql: &str) -> RS<StmtAck> {
        let prefix = "ack";
        let rest = sql[prefix.len()..].trim();
        let mut tokens = rest.split_whitespace();
        let name = tokens
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "ack must specify a queue name"))?;
        validate_type_name(name)?;
        let mut consumer: Option<String> = None;
        let mut to = tokens
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "ack must contain TO <seq>"))?;
        if to.eq_ignore_ascii_case("for") {
            let name = tokens.next().ok_or_else(|| {
                mudu_error!(ErrorCode::Parse, "ack FOR requires a consumer cursor name")
            })?;
            validate_type_name(name)?;
            consumer = Some(name.to_string());
            to = tokens
                .next()
                .ok_or_else(|| mudu_error!(ErrorCode::Parse, "ack must contain TO <seq>"))?;
        }
        if !to.eq_ignore_ascii_case("to") {
            return Err(mudu_error!(ErrorCode::Parse, "ack must contain TO <seq>"));
        }
        let seq_token = tokens
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "ack must specify a sequence number"))?;
        let seq = parse_u64_token(seq_token, "ack sequence number")?;
        if let Some(extra) = tokens.next() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!("unexpected trailing token {extra} in ack statement")
            ));
        }
        let mut stmt_ack = StmtAck::new(name.to_string(), seq);
        if let Some(consumer) = consumer {
            stmt_ack.set_consumer(consumer);
        }
        Ok(stmt_ack)
    }

    /// Parse a `CREATE CONSUMER <consumer> ON <queue>` statement.
    pub(crate) fn parse_create_consumer_custom(&self, sql: &str) -> RS<StmtCreateConsumer> {
        let (consumer, queue) = parse_consumer_on_queue(sql, "create consumer")?;
        Ok(StmtCreateConsumer::new(consumer, queue))
    }

    /// Parse a `DROP CONSUMER <consumer> ON <queue>` statement.
    pub(crate) fn parse_drop_consumer_custom(&self, sql: &str) -> RS<StmtDropConsumer> {
        let (consumer, queue) = parse_consumer_on_queue(sql, "drop consumer")?;
        Ok(StmtDropConsumer::new(consumer, queue))
    }
}

/// Parses the optional `PARTITION BY GLOBAL RULE ... REFERENCES (...)`
/// sharding clause of a `CREATE QUEUE` statement; an empty tail yields
/// `None`, anything else must be a well-formed clause.
fn parse_queue_partition_tail(tail: &str) -> RS<Option<StmtTablePartition>> {
    if tail.is_empty() {
        return Ok(None);
    }
    Ok(Some(super::partition::parse_table_partition_suffix(tail)?))
}

/// Parse the shared `<consumer> ON <queue>` tail of the consumer cursor DDL
/// statements.
fn parse_consumer_on_queue(sql: &str, prefix: &str) -> RS<(String, String)> {
    let rest = sql[prefix.len()..].trim();
    let mut tokens = rest.split_whitespace();
    let consumer = tokens.next().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            format!("{prefix} must specify a consumer cursor name")
        )
    })?;
    validate_type_name(consumer)?;
    let on = tokens.next().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            format!("{prefix} must contain ON <queue>")
        )
    })?;
    if !on.eq_ignore_ascii_case("on") {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("{prefix} must contain ON <queue>")
        ));
    }
    let queue = tokens.next().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            format!("{prefix} must specify a queue name")
        )
    })?;
    validate_type_name(queue)?;
    if let Some(extra) = tokens.next() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("unexpected trailing token {extra} in {prefix} statement")
        ));
    }
    Ok((consumer.to_string(), queue.to_string()))
}
