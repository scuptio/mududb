//! The universal SQL-literal filter and the shared extraction types.

use sql_parser::check::param_type::ParamTypeTag;

/// A SQL string literal extracted from a guest source file, together with
/// the statically known call-site context used by the enhancement checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractedSql {
    /// The literal content (unquoted; for Rust, unescaped by `syn`).
    pub sql: String,
    /// 1-based line of the literal token in the source file.
    pub line: usize,
    /// 1-based byte column of the literal token in the source file.
    pub column: usize,
    /// Statically known number of bind parameters supplied at the call
    /// site, when the literal is passed to a query/command call together
    /// with a tuple-style parameter list.
    pub param_arity: Option<usize>,
    /// Entity type name from the call's turbofish
    /// (`mudu_query::<Wallets>`), when present.
    pub entity: Option<String>,
    /// Per-position type tags of the parameter tuple elements that are
    /// source literals (`42`, `0L`, `"s"`, `1.5`, `true`, `b".."`);
    /// `None` for non-literal elements. Empty when the parameter shape is
    /// not statically known.
    pub param_literal_tags: Vec<Option<ParamTypeTag>>,
}

impl ExtractedSql {
    /// Create an extracted literal without call-site context.
    pub fn new(sql: String, line: usize, column: usize) -> Self {
        Self {
            sql,
            line,
            column,
            param_arity: None,
            entity: None,
            param_literal_tags: Vec::new(),
        }
    }
}

/// Return the lower-cased DML keyword when `content` (after trimming
/// leading whitespace) starts a DML statement (ASCII case-insensitive).
///
/// Recognition is keyword-pair based so natural-language strings that
/// merely start with a word like "Insert" (e.g. `"Insert user"`) are not
/// treated as SQL:
///
/// - `INSERT` must be followed by `INTO`;
/// - `DELETE` must be followed by `FROM`;
/// - `UPDATE` must contain a `SET` word later in the text;
/// - `SELECT` accepts any following token (the select list is arbitrary).
///
/// In every case the keyword must sit on a word boundary and be followed
/// by at least one more token: a bare keyword fragment (e.g. the
/// `"UPDATE "` pieces produced by entity-generated dynamic-SQL builders)
/// is not a statement and returns `None`.
pub fn dml_keyword(content: &str) -> Option<&'static str> {
    let trimmed = content.trim_start();
    for keyword in ["select", "insert", "update", "delete"] {
        let Some(rest) = keyword_prefix_rest(trimmed, keyword) else {
            continue;
        };
        let follows = match keyword {
            "insert" => keyword_prefix_rest(rest, "into").is_some(),
            "delete" => keyword_prefix_rest(rest, "from").is_some(),
            "update" => contains_word(rest, "set"),
            _ => true,
        };
        if follows {
            return Some(keyword);
        }
        return None;
    }
    None
}

/// When `text` (after trimming leading whitespace) starts with `keyword`
/// (ASCII case-insensitive) on a word boundary and with at least one more
/// token after it, return the text following the keyword; otherwise
/// return `None`.
fn keyword_prefix_rest<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let text = text.trim_start();
    if text.len() < keyword.len() {
        return None;
    }
    let head = &text[..keyword.len()];
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let rest = &text[keyword.len()..];
    match rest.chars().next() {
        // A lone keyword is a fragment, not a statement.
        None => None,
        // Word characters mean a longer identifier (`Selects`, ...).
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => None,
        Some(_) => {
            if rest.trim().is_empty() {
                None
            } else {
                Some(rest)
            }
        }
    }
}

/// True when `word` appears in `text` on word boundaries (ASCII
/// case-insensitive).
fn contains_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .any(|token| token.eq_ignore_ascii_case(word))
}

/// True when the literal content should be checked as SQL.
///
/// Literals with `{`/`}` holes are `format!`-style templates (their final
/// text is built at runtime) and are skipped.
pub fn is_sql_candidate(content: &str) -> bool {
    if content.contains('{') || content.contains('}') {
        return false;
    }
    dml_keyword(content).is_some()
}
