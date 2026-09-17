//! Golden tests for Rust entity code generation from DDL.
//!
//! The golden files under `tests/golden/` pin the full text produced by
//! [`CodeGen::generate_entity_code_from_ddl_sql`]; any drift in the
//! `rust/entity.rs.jinja` template output fails this test.
//!
//! When a template change is intentional, regenerate the goldens with
//! `mgen entity` using the DDL below and re-inspect the diff.

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use mudu_gen::src_gen::code_gen::CodeGen;
    use std::path::PathBuf;

    /// (a) Single-primary-key table mixing `NOT NULL` and nullable columns.
    /// (b) Composite-primary-key table.
    const DDL: &str = "\
CREATE TABLE item (
    i_id INT PRIMARY KEY,
    i_name VARCHAR(100),
    i_price DOUBLE NOT NULL,
    i_data VARCHAR(100),
    i_im_id INT
);
CREATE TABLE order_line (
    ol_o_id INT PRIMARY KEY,
    ol_d_id INT PRIMARY KEY,
    ol_number INT PRIMARY KEY,
    ol_amount DOUBLE,
    ol_quantity INT NOT NULL
);
";

    fn golden_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("golden")
            .join(name)
    }

    fn read_golden(name: &str) -> String {
        mudu_sys::fs::sync::sync_read_to_string(golden_path(name)).unwrap()
    }

    // Miri cannot execute FFI calls into the tree-sitter C parser, so skip
    // under Miri; the test runs under normal `cargo test`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn entity_generation_matches_golden() {
        let result = CodeGen::generate_entity_code_from_ddl_sql(DDL, "rust", false).unwrap();
        assert_eq!(result.source_code.len(), 2);
        for (table, golden) in [
            ("item", "item.entity.rs"),
            ("order_line", "order_line.entity.rs"),
        ] {
            let generated = result
                .source_code
                .get(table)
                .unwrap_or_else(|| panic!("no entity generated for table {table}"));
            let expected = read_golden(golden);
            assert_eq!(
                generated.trim_end(),
                expected.trim_end(),
                "entity output for table `{table}` drifted from tests/golden/{golden}; \
                 regenerate the golden if the template change is intentional"
            );
        }
    }
}
