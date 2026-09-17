//! Process-wide cache for parsed SQL statements.
//!
//! `tree-sitter` parsing costs ~30µs per statement (see
//! `sql_parser/tests/parse_bench.rs`), and benchmark workloads re-execute a
//! small set of parameterized statement templates thousands of times per
//! second. The parse output depends only on the SQL text, so the result can
//! be shared process-wide without any schema invalidation. Binding and
//! planning are intentionally NOT cached here: they depend on the catalog
//! and would need DDL-aware invalidation.
//!
//! The cache is sharded (`STMT_PARSE_CACHE_SHARDS` independently locked maps,
//! selected by an FNV-1a hash of the SQL text) so concurrent workers do not
//! serialize on one global lock for every statement.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use sql_parser::ast::stmt_type::StmtType;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Upper bound on cached statements; beyond it new entries are simply not
/// inserted (pathological workloads with millions of distinct SQL texts
/// fall back to parsing every time).
const STMT_PARSE_CACHE_CAPACITY: usize = 4096;

/// Number of independently locked shards. Every statement lookup used to take
/// one process-wide lock, so 8 worker threads parsing the same hot templates
/// serialized on it; sharding by SQL text hash spreads the hot entries across
/// `STMT_PARSE_CACHE_SHARDS` locks. Fixed at 16 (2x the typical 8 worker
/// threads): large enough to make shard collisions rare, small enough that the
/// static array stays trivial.
const STMT_PARSE_CACHE_SHARDS: usize = 16;

/// Per-shard capacity. The total bound of `STMT_PARSE_CACHE_CAPACITY` entries
/// is preserved by splitting it evenly across shards; a shard that fills up
/// simply stops accepting new entries, same policy as before.
const STMT_PARSE_CACHE_SHARD_CAPACITY: usize = STMT_PARSE_CACHE_CAPACITY / STMT_PARSE_CACHE_SHARDS;

/// FNV-1a over the SQL text. Only used for shard selection (the `HashMap`
/// re-hashes with its own hasher for the bucket lookup), so a cheap
/// byte-stream hash keeps the extra pass over the SQL text near 1 cycle/byte
/// instead of paying SipHash twice per statement.
fn shard_index(sql: &str) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash as usize) % STMT_PARSE_CACHE_SHARDS
}

struct StmtParseCache {
    shards: [SMutex<HashMap<String, Arc<StmtType>>>; STMT_PARSE_CACHE_SHARDS],
}

impl StmtParseCache {
    fn new() -> Self {
        Self {
            shards: std::array::from_fn(|_| SMutex::new(HashMap::new())),
        }
    }

    fn get(&self, sql: &str) -> RS<Option<Arc<StmtType>>> {
        Ok(self.shards[shard_index(sql)]
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "stmt parse cache lock poisoned"))?
            .get(sql)
            .cloned())
    }

    fn insert(&self, sql: String, stmt: Arc<StmtType>) -> RS<()> {
        let mut guard = self.shards[shard_index(&sql)]
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "stmt parse cache lock poisoned"))?;
        if guard.len() < STMT_PARSE_CACHE_SHARD_CAPACITY {
            guard.insert(sql, stmt);
        }
        Ok(())
    }
}

fn stmt_parse_cache() -> &'static StmtParseCache {
    static CACHE: OnceLock<StmtParseCache> = OnceLock::new();
    CACHE.get_or_init(StmtParseCache::new)
}

/// Returns a cached parse of `sql` when present, otherwise parses with
/// `parse`, caches the result, and returns it.
///
/// The statement is shared behind an `Arc`: a cache hit costs an atomic
/// refcount bump instead of a deep copy of the whole AST. Consumers must
/// treat the statement as read-only (binding only borrows it).
pub(crate) fn parse_one_cached<F>(sql: &str, parse: F) -> RS<Arc<StmtType>>
where
    F: FnOnce(&str) -> RS<StmtType>,
{
    if let Some(stmt) = stmt_parse_cache().get(sql)? {
        return Ok(stmt);
    }
    let stmt = Arc::new(parse(sql)?);
    stmt_parse_cache().insert(sql.to_string(), stmt.clone())?;
    Ok(stmt)
}

// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// used by SQLParser inside this module. Tests that parse SQL are skipped
// under Miri; their behavior is still exercised by normal `cargo test`.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn parse_select(sql: &str) -> RS<StmtType> {
        sql_parser::ast::parser::SQLParser::new()?
            .parse(sql)?
            .into_stmts()
            .into_iter()
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "no statement parsed"))
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn cached_parse_returns_same_statement_and_parses_once() {
        let sql = "SELECT a FROM t WHERE b = ?";
        let parse_calls = AtomicUsize::new(0);
        let parse = |sql: &str| {
            parse_calls.fetch_add(1, Ordering::SeqCst);
            parse_select(sql)
        };
        // Bypass the process-wide cache assertions by using a unique SQL text.
        let unique_sql = format!("{sql} /* test {} */", line!());
        let first = parse_one_cached(&unique_sql, parse).unwrap();
        let second = parse_one_cached(&unique_sql, parse).unwrap();
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
        assert_eq!(parse_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn parse_errors_are_not_cached() {
        let unique_sql = format!("NOT VALID SQL /* test {} */", line!());
        for _ in 0..2 {
            let result = parse_one_cached(&unique_sql, parse_select);
            assert!(result.is_err());
        }
        assert!(stmt_parse_cache().get(&unique_sql).unwrap().is_none());
    }

    #[test]
    fn shard_index_is_deterministic_and_bounded() {
        for sql in [
            "SELECT 1",
            "INSERT INTO t VALUES (?)",
            "",
            "SELECT a FROM t",
        ] {
            let index = shard_index(sql);
            assert_eq!(index, shard_index(sql));
            assert!(index < STMT_PARSE_CACHE_SHARDS);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn concurrent_hits_on_same_sql_share_one_statement() {
        let unique_sql = format!("SELECT a FROM t WHERE b = ? /* test {} */", line!());
        // Warm the cache so every thread below is a cache reader.
        let expected = parse_one_cached(&unique_sql, parse_select).unwrap();
        let mut handles = Vec::new();
        for _ in 0..8 {
            let sql = unique_sql.clone();
            handles.push(
                mudu_sys::task::sync::spawn_thread(move || {
                    let mut last = None;
                    for _ in 0..100 {
                        last = Some(parse_one_cached(&sql, parse_select).unwrap());
                    }
                    last.unwrap()
                })
                .unwrap(),
            );
        }
        for handle in handles {
            let stmt = handle.join().unwrap();
            assert!(Arc::ptr_eq(&expected, &stmt));
        }
    }
}
