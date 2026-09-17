//! Per-application schema namespace mapping.
//!
//! Every table lives in a schema (PostgreSQL-schema-like namespace). The
//! schema name is the application name from `package.cfg.json`; requests that
//! carry no application context (or the sentinel names `""` / `"default"`)
//! resolve against the default schema [`DEFAULT_SCHEMA`]. Unqualified table
//! names resolve through the search path `[current_schema, mududb]`: an exact
//! hit in the current schema wins; on a miss the lookup falls back to the
//! default schema (implemented inside `MetaMgr::get_table_by_name`).

/// Name of the default schema that holds tables created without an
/// application context (the PostgreSQL `public` analogue).
pub const DEFAULT_SCHEMA: &str = "mududb";

/// Map an application name (as carried by `ClientRequest.app_name` or the
/// `app=` connection-string option) to the schema the request resolves
/// against: `None`, `""` and `"default"` map to [`DEFAULT_SCHEMA`]; any other
/// name is used verbatim (dashes kept, e.g. `wallet-go`).
pub fn current_schema(app_name: Option<&str>) -> String {
    match app_name {
        None | Some("") | Some("default") => DEFAULT_SCHEMA.to_string(),
        Some(name) => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn current_schema_maps_sentinel_app_names_to_default_schema() {
        assert_eq!(current_schema(None), DEFAULT_SCHEMA);
        assert_eq!(current_schema(Some("")), DEFAULT_SCHEMA);
        assert_eq!(current_schema(Some("default")), DEFAULT_SCHEMA);
    }

    #[test]
    fn current_schema_keeps_app_name_verbatim() {
        assert_eq!(current_schema(Some("wallet")), "wallet");
        assert_eq!(current_schema(Some("wallet-go")), "wallet-go");
    }
}
