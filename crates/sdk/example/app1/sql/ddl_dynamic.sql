-- Table created at runtime by the proc_sys_call_mtp procedure itself (see
-- src/rust/procedures.rs), not by sql/ddl.sql at package install time. The
-- static SQL checker (mgen check-sql) reads this file together with
-- ddl.sql so statements against wallets resolve their schema.

CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);
