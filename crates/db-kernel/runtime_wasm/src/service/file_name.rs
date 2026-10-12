/// Name of the application configuration file inside an `.mpk` archive.
pub const PACKAGE_CFG: &str = "package.cfg.json";
/// Name of the module/procedure descriptor file inside an `.mpk` archive.
pub const PROCEDURE_DESC: &str = "package.desc.json";
/// Name of the optional package manifest file inside an `.mpk` archive.
pub const PACKAGE_MANIFEST: &str = "package.manifest.json";
/// Name of the schema DDL file inside an `.mpk` archive.
pub const DDL_SQL: &str = "ddl.sql";
/// Name of the initialization SQL file inside an `.mpk` archive.
pub const INIT_DB_SQL: &str = "initdb.sql";
/// File-name suffix of a WebAssembly bytecode module inside an `.mpk`
/// archive.
pub const BYTE_CODE_MOD_SUFFIX: &str = ".wasm";

/// File extension of a Mudu application package archive.
pub const APP_PACKAGE_EXTENSION: &str = "mpk";
