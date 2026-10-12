//! Full-database logical dump / restore.
//!
//! A logical dump packages the catalog metadata (tables, partition rules,
//! partition bindings/placements, filesystem types) together with one CSV
//! file per table; a logical restore reconstructs the schema and reloads the
//! data. The authoritative schema input is a structured, msgpack-encoded
//! `Vec<SchemaTable>` (`schema.mp`) so the round-trip is lossless; the
//! rendered `schema.sql` is a human-readable / portable attachment.
//!
//! The data path reuses the existing `COPY TO` / `COPY FROM` commands
//! (`SaveToFile` / `LoadFromFile`), so CSV encoding is identical to the SQL
//! `COPY` surface.

use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::fs_type::{FsColumnBinding, FsTypeDesc};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::PartitionRuleDesc;
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::command::load_from_file::{LoadFromFile, LoadFromFileParams};
use ::command::save_to_file::{SaveToFile, SaveToFileParams};
use ::meta::fs_object::FS_OBJECT_TABLE_ID;
use ::sql::copy_layout::CopyLayout;
use ::worker::worker::WorkerRuntime;
use ::xengine::api::XContract;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::default_sys_io_context;
use mudu_type::type_family::TypeFamily;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::warn;

/// Manifest file name inside a logical dump archive.
const MANIFEST_FILE: &str = "manifest.mp";
/// Structured schema file name (msgpack `Vec<SchemaTable>`).
const SCHEMA_FILE: &str = "schema.mp";
/// Human-readable DDL file name.
const SCHEMA_SQL_FILE: &str = "schema.sql";
/// Per-table CSV directory name.
const DATA_DIR: &str = "data";
/// Archive file name suffix.
const ARCHIVE_SUFFIX: &str = ".zip";

/// A table reference inside a logical dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LogicalTableRef {
    /// Schema (namespace) the table belongs to.
    pub schema: String,
    /// Bare table name.
    pub name: String,
    /// CSV file name under `data/`.
    pub csv_file: String,
    /// Row count written at dump time.
    pub rows: u64,
}

/// A partition placement keyed by the source worker index (worker ids are
/// per-server and must be re-mapped on restore).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LogicalPlacement {
    /// Partition id (an OID preserved across dump/restore).
    pub partition_id: OID,
    /// Source worker index.
    pub worker_index: u32,
}

/// The self-sealing manifest of a logical dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LogicalDumpManifest {
    /// Dump label (the directory name under `dir`).
    pub label: String,
    /// Wall-clock time the dump was taken (millis since the epoch).
    pub taken_at_ms: u64,
    /// Referenced tables (system tables excluded).
    pub tables: Vec<LogicalTableRef>,
    /// Registered filesystem types.
    pub fs_types: Vec<FsTypeDesc>,
    /// Partition rules.
    pub partition_rules: Vec<PartitionRuleDesc>,
    /// Table partition bindings.
    pub partition_bindings: Vec<TablePartitionBinding>,
    /// Partition placements (worker-index keyed).
    pub placements: Vec<LogicalPlacement>,
}

/// Outcome of a logical dump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogicalDumpReport {
    /// Dump label.
    pub label: String,
    /// Number of tables dumped.
    pub tables: u64,
    /// Total rows written.
    pub rows: u64,
}

/// Outcome of a logical restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LogicalRestoreReport {
    /// Dump label restored.
    pub label: String,
    /// Tables created.
    pub tables_created: u64,
    /// Tables skipped (already present and not `--drop`).
    pub tables_skipped: u64,
    /// Total rows loaded.
    pub rows_loaded: u64,
}

/// The dump directory of one label (temporary staging directory).
fn dump_label_dir(dir: &str, label: &str) -> PathBuf {
    PathBuf::from(dir).join(label)
}

/// The archive file of one label (`dir/<label>.zip`).
fn archive_path(dir: &str, label: &str) -> PathBuf {
    PathBuf::from(dir).join(format!("{label}{ARCHIVE_SUFFIX}"))
}

/// The CSV file name of a table.
fn table_csv_file(schema: &str, name: &str) -> String {
    format!("{schema}.{name}.csv")
}

/// The CSV path of a table inside a dump directory.
fn table_csv_path(dir: &str, label: &str, schema: &str, name: &str) -> PathBuf {
    dump_label_dir(dir, label)
        .join(DATA_DIR)
        .join(table_csv_file(schema, name))
}

/// Writes `bytes` to `path` synchronously with an fsync.
fn write_file_sync(path: &Path, bytes: &[u8]) -> RS<()> {
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::create_dir_all(parent)?;
    }
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.write(true).create(true).truncate(true);
    let mut file = options.open(path)?;
    file.write_all(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "write logical dump file error", e))?;
    file.sync_data()?;
    Ok(())
}

/// Reads a whole file into memory (sync).
fn read_file_sync(path: &Path) -> RS<Vec<u8>> {
    mudu_sys::fs::sync::read(path)
        .map_err(|e| mudu_error!(ErrorCode::Io, "read logical dump file error", e))
}

/// Packs the staged dump directory into a single ZIP archive.
fn pack_dir_to_zip(stage_dir: &Path, zip_path: &Path, csv_files: &[String]) -> RS<()> {
    let file = mudu_sys::fs::sync::SFile::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for name in [MANIFEST_FILE, SCHEMA_FILE, SCHEMA_SQL_FILE] {
        let bytes = read_file_sync(&stage_dir.join(name))?;
        zip.start_file(name, options)
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip start file error", e))?;
        zip.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip write error", e))?;
    }
    for csv_file in csv_files {
        let name = format!("{DATA_DIR}/{csv_file}");
        let bytes = read_file_sync(&stage_dir.join(DATA_DIR).join(csv_file))?;
        zip.start_file(&name, options)
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip start file error", e))?;
        zip.write_all(&bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip write error", e))?;
    }
    zip.finish()
        .map_err(|e| mudu_error!(ErrorCode::Io, "zip finish error", e))?;
    Ok(())
}

/// Extracts a ZIP archive into `stage_dir`.
fn extract_zip_to_dir(zip_path: &Path, stage_dir: &Path) -> RS<()> {
    mudu_sys::fs::sync::create_dir_all(stage_dir)?;
    let file = mudu_sys::fs::sync::SFile::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "open logical dump archive error", e))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "zip archive by_index error", e))?;
        let name = entry
            .name()
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip walk error", e))?
            .to_string();
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip read error", e))?;
        let dest = stage_dir.join(&name);
        write_file_sync(&dest, &bytes)?;
    }
    Ok(())
}

/// Reads one entry of a ZIP archive into memory.
fn read_zip_entry(zip_path: &Path, name: &str) -> RS<Option<Vec<u8>>> {
    let file = mudu_sys::fs::sync::SFile::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "open logical dump archive error", e))?;
    let Ok(mut entry) = archive.by_name(name) else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "zip read error", e))?;
    Ok(Some(bytes))
}

// === DDL rendering ========================================================

/// Renders the SQL type name of one column, resolving filesystem-type
/// bindings back to their registered type name.
fn sql_column_type(col: &SchemaColumn, fs_types: &[FsTypeDesc]) -> String {
    if let Some(binding) = col.fs_binding() {
        return fs_types
            .iter()
            .find(|desc| desc.fs_id() == binding.fs_id())
            .map(|desc| desc.name().to_string())
            .unwrap_or_else(|| "oid".to_string());
    }
    let data_type = col.type_param().to_data_type().ok();
    match col.type_id() {
        TypeFamily::I32 => "INT".to_string(),
        TypeFamily::I64 => "BIGINT".to_string(),
        TypeFamily::F32 => "FLOAT".to_string(),
        TypeFamily::F64 => "DOUBLE".to_string(),
        TypeFamily::String => data_type
            .and_then(|ty| ty.as_string_param().map(|param| param.length()))
            .filter(|len| *len > 0)
            .map(|len| format!("VARCHAR({len})"))
            .unwrap_or_else(|| "TEXT".to_string()),
        TypeFamily::Numeric => data_type
            .and_then(|ty| ty.as_numeric_param().map(|p| (p.precision(), p.scale())))
            .map(|(precision, scale)| format!("NUMERIC({precision}, {scale})"))
            .unwrap_or_else(|| "NUMERIC".to_string()),
        TypeFamily::Date => "DATE".to_string(),
        TypeFamily::Time => "TIME".to_string(),
        TypeFamily::Timestamp => "TIMESTAMP".to_string(),
        TypeFamily::TimestampTz => "TIMESTAMPTZ".to_string(),
        // Rare / complex families: the structured schema is authoritative, so
        // render a readable placeholder for the SQL attachment.
        _ => "TEXT".to_string(),
    }
}

/// Renders the `CREATE TABLE` DDL of one table. The result is a human-readable
/// attachment; the authoritative schema input is the msgpack `schema.mp`.
pub(crate) fn render_create_table_ddl(schema: &SchemaTable, fs_types: &[FsTypeDesc]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "CREATE TABLE {}.{} (\n",
        schema.schema(),
        schema.table_name()
    ));
    let primary: Vec<&str> = schema
        .key_columns()
        .iter()
        .map(|col| col.get_name().as_str())
        .collect();
    for (index, col) in schema.columns().iter().enumerate() {
        let ty = sql_column_type(col, fs_types);
        let not_null = if col.is_primary() || !col.nullable() {
            " NOT NULL"
        } else {
            ""
        };
        out.push_str(&format!("  {} {}{}", col.get_name(), ty, not_null));
        if index + 1 < schema.columns().len() {
            out.push(',');
        }
        out.push('\n');
    }
    if !primary.is_empty() {
        out.push_str(&format!("  PRIMARY KEY ({})\n", primary.join(", ")));
    }
    out.push_str(");\n");
    out
}

/// Renders the whole `schema.sql` attachment: one DDL per table.
fn render_schema_sql(schemas: &[SchemaTable], fs_types: &[FsTypeDesc]) -> String {
    schemas
        .iter()
        .map(|schema| render_create_table_ddl(schema, fs_types))
        .collect::<Vec<_>>()
        .join("\n")
}

// === dump ================================================================

/// Loads the meta manager of the default worker (the catalog rides on
/// partition 0).
fn default_meta_mgr(
    workers: &[(&WorkerRuntime, u32)],
) -> RS<(Arc<dyn MetaMgr>, Arc<dyn XContract>)> {
    let worker = workers
        .first()
        .map(|(worker, _)| *worker)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "no workers registered"))?;
    Ok((worker.meta_mgr(), worker.x_contract()))
}

/// Catalog metadata gathered for a logical dump: the filtered table schemas,
/// the fs-type / partition-rule / binding sets, the placements re-keyed to
/// worker indices, and the set of partitioned table ids.
struct DumpCatalog {
    schemas: Vec<SchemaTable>,
    fs_types: Vec<FsTypeDesc>,
    partition_rules: Vec<PartitionRuleDesc>,
    partition_bindings: Vec<TablePartitionBinding>,
    placements: Vec<LogicalPlacement>,
    partitioned_tables: std::collections::HashSet<OID>,
}

/// Enumerates the catalog metadata of a dump and re-maps the placement
/// worker ids to worker indices for the manifest.
async fn gather_dump_catalog(
    meta_mgr: &Arc<dyn MetaMgr>,
    workers: &[(Arc<WorkerRuntime>, u32)],
) -> RS<DumpCatalog> {
    let schemas = meta_mgr.list_schemas().await?;
    let schemas: Vec<SchemaTable> = schemas
        .into_iter()
        .filter(|schema| schema.id() != FS_OBJECT_TABLE_ID)
        .collect();
    let fs_types = meta_mgr.list_fs_types().await?;
    let partition_rules = meta_mgr.list_partition_rules().await?;
    let partition_bindings = meta_mgr.list_partition_bindings().await?;
    let placements = meta_mgr.list_partition_placements().await?;

    // Map source worker ids to worker indices for the placement manifest.
    let mut worker_id_to_index = HashMap::new();
    for (worker, index) in workers {
        worker_id_to_index.insert(worker.worker_id(), *index);
    }
    let placements: Vec<LogicalPlacement> = placements
        .into_iter()
        .map(|placement| LogicalPlacement {
            partition_id: placement.partition_id,
            worker_index: worker_id_to_index
                .get(&placement.worker_id)
                .copied()
                .unwrap_or(0),
        })
        .collect();

    let partitioned_tables: std::collections::HashSet<OID> = partition_bindings
        .iter()
        .map(|binding| binding.table_id)
        .collect();

    Ok(DumpCatalog {
        schemas,
        fs_types,
        partition_rules,
        partition_bindings,
        placements,
        partitioned_tables,
    })
}

/// Writes one CSV per non-partitioned table under the dump staging directory,
/// reading every table through one snapshot transaction; partitioned-table
/// data is skipped (metadata only). Returns the manifest table references.
async fn dump_table_csvs(
    dir: &str,
    label: &str,
    catalog: &DumpCatalog,
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
) -> RS<Vec<LogicalTableRef>> {
    // Take one snapshot for every table read.
    let tx_mgr = x_contract.begin_tx().await?;
    let async_runtime = default_sys_io_context().provider_arc();

    let mut tables = Vec::with_capacity(catalog.schemas.len());
    for schema in &catalog.schemas {
        let is_partitioned = catalog.partitioned_tables.contains(&schema.id());
        let csv_file = table_csv_file(schema.schema(), schema.table_name());
        let csv_path = table_csv_path(dir, label, schema.schema(), schema.table_name());
        let rows = if is_partitioned {
            warn!(
                table = %schema.table_name(),
                "logical dump: skipping partitioned-table data (metadata only)"
            );
            0
        } else {
            let table_desc = meta_mgr.get_table_by_id(schema.id()).await?;
            let layout = CopyLayout::new(&table_desc, &[])?;
            let cmd = SaveToFile::new(SaveToFileParams {
                file_path: csv_path.to_string_lossy().to_string(),
                tx_mgr: tx_mgr.clone(),
                table_id: table_desc.id(),
                key_indexing: layout.key_index().to_vec(),
                value_indexing: layout.value_index().to_vec(),
                x_contract: x_contract.clone(),
                meta_mgr: meta_mgr.clone(),
                async_runtime: Some(async_runtime.clone()),
            });
            cmd.prepare().await?;
            cmd.run().await?;
            cmd.affected_rows().await?
        };
        tables.push(LogicalTableRef {
            schema: schema.schema().clone(),
            name: schema.table_name().clone(),
            csv_file,
            rows,
        });
    }
    x_contract.commit_tx(tx_mgr).await?;
    Ok(tables)
}

/// Publishes a dump: writes the structured schema, the DDL attachment and the
/// manifest into the staging directory, packs the staged directory into a
/// single ZIP archive, and removes the staging directory (a dump is a
/// self-contained archive file).
fn publish_dump(
    dir: &str,
    label: &str,
    schemas: &[SchemaTable],
    manifest: &LogicalDumpManifest,
) -> RS<()> {
    let label_dir = dump_label_dir(dir, label);
    let schema_bytes = rmp_serde::to_vec(schemas)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode logical dump schema error", e))?;
    write_file_sync(&label_dir.join(SCHEMA_FILE), &schema_bytes)?;
    let schema_sql = render_schema_sql(schemas, &manifest.fs_types);
    write_file_sync(&label_dir.join(SCHEMA_SQL_FILE), schema_sql.as_bytes())?;
    let manifest_bytes = rmp_serde::to_vec(manifest)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode logical dump manifest error", e))?;
    write_file_sync(&label_dir.join(MANIFEST_FILE), &manifest_bytes)?;

    let csv_files: Vec<String> = manifest.tables.iter().map(|t| t.csv_file.clone()).collect();
    let archive = archive_path(dir, label);
    pack_dir_to_zip(&label_dir, &archive, &csv_files)?;
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&label_dir);
    Ok(())
}

/// Dumps the whole server: enumerates the catalog, writes the manifest, the
/// structured schema, the DDL attachment, and one CSV per non-partitioned
/// table. Partitioned-table data is skipped (cross-worker consistent reads
/// are not implemented); their metadata is still dumped.
pub async fn run_logical_dump(
    server_instance_id: ServerInstanceId,
    dir: &str,
    label: &str,
) -> RS<LogicalDumpReport> {
    let workers = crate::backup::backup_workers(server_instance_id)?;
    if workers.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!("server {server_instance_id} has no registered workers")
        ));
    }
    let worker_refs: Vec<(&WorkerRuntime, u32)> = workers
        .iter()
        .map(|(worker, index)| (worker.as_ref(), *index))
        .collect();
    let (meta_mgr, x_contract) = default_meta_mgr(&worker_refs)?;
    let catalog = gather_dump_catalog(&meta_mgr, &workers).await?;

    // Create the dump directories up front (the CSV writer does not create
    // parent directories).
    mudu_sys::fs::sync::create_dir_all(dump_label_dir(dir, label).join(DATA_DIR))?;

    let tables = dump_table_csvs(dir, label, &catalog, &meta_mgr, &x_contract).await?;

    // Publish the dump: schema first, manifest last (a dump is valid only
    // once its manifest decodes and every referenced CSV exists).
    let manifest = LogicalDumpManifest {
        label: label.to_string(),
        taken_at_ms: mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        tables,
        fs_types: catalog.fs_types,
        partition_rules: catalog.partition_rules,
        partition_bindings: catalog.partition_bindings,
        placements: catalog.placements,
    };
    publish_dump(dir, label, &catalog.schemas, &manifest)?;

    Ok(LogicalDumpReport {
        label: label.to_string(),
        tables: manifest.tables.len() as u64,
        rows: manifest.tables.iter().map(|table| table.rows).sum(),
    })
}

// === restore =============================================================

/// Loads a logical dump manifest from its archive.
pub(crate) fn load_logical_dump(dir: &str, label: &str) -> RS<Option<LogicalDumpManifest>> {
    let path = archive_path(dir, label);
    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(None);
    }
    let Some(bytes) = read_zip_entry(&path, MANIFEST_FILE)? else {
        return Ok(None);
    };
    match rmp_serde::from_slice::<LogicalDumpManifest>(&bytes) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(_) => Ok(None),
    }
}

/// Loads the structured schema of a logical dump from its archive.
fn load_logical_schema(dir: &str, label: &str) -> RS<Vec<SchemaTable>> {
    let path = archive_path(dir, label);
    let bytes = read_zip_entry(&path, SCHEMA_FILE)?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::NotFound,
            format!("logical dump {label} has no schema entry")
        )
    })?;
    rmp_serde::from_slice(&bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode logical dump schema error", e))
}

/// Restores a logical dump into the live server. `drop_existing` drops and
/// recreates a same-named table; otherwise already-present tables are skipped
/// (default: create only missing tables).
pub async fn run_logical_restore(
    server_instance_id: ServerInstanceId,
    dir: &str,
    label: &str,
    drop_existing: bool,
) -> RS<LogicalRestoreReport> {
    let workers = crate::backup::backup_workers(server_instance_id)?;
    if workers.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!("server {server_instance_id} has no registered workers")
        ));
    }
    let worker_refs: Vec<(&WorkerRuntime, u32)> = workers
        .iter()
        .map(|(worker, index)| (worker.as_ref(), *index))
        .collect();
    let (meta_mgr, x_contract) = default_meta_mgr(&worker_refs)?;
    let manifest = load_logical_dump(dir, label)?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such logical dump {label}")
        )
    })?;
    let schemas = load_logical_schema(dir, label)?;

    // Extract the archive into a staging directory so the CSV files can be
    // streamed through `LoadFromFile`; the manifest and schema are read
    // directly from the archive above.
    let label_dir = dump_label_dir(dir, label);
    extract_zip_to_dir(&archive_path(dir, label), &label_dir)?;

    let fs_id_map = restore_fs_types(&meta_mgr, &manifest.fs_types).await?;
    let restored =
        restore_tables(&meta_mgr, &x_contract, &schemas, &fs_id_map, drop_existing).await?;
    restore_partitions(&meta_mgr, &workers, &manifest).await?;
    let rows_loaded = load_table_data(
        dir,
        label,
        &manifest,
        &schemas,
        &restored.names,
        &meta_mgr,
        &x_contract,
    )
    .await?;

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&label_dir);

    Ok(LogicalRestoreReport {
        label: label.to_string(),
        tables_created: restored.created,
        tables_skipped: restored.skipped,
        rows_loaded,
    })
}

/// Tables created / skipped by the DDL phase of a restore.
struct RestoredTables {
    /// Tables created.
    created: u64,
    /// Tables skipped (already present and not dropped).
    skipped: u64,
    /// `(schema, table)` names created during this restore.
    names: std::collections::HashSet<(String, String)>,
}

/// Re-maps the dump's fs ids to the target server's fs types by name,
/// creating the fs types the target server does not have yet.
async fn restore_fs_types(
    meta_mgr: &Arc<dyn MetaMgr>,
    fs_types: &[FsTypeDesc],
) -> RS<HashMap<u64, u64>> {
    // Filesystem types: ids are allocated per-server, so re-map the dump's
    // fs ids to the newly-created ones by name.
    let mut fs_id_map: HashMap<u64, u64> = HashMap::new();
    for desc in fs_types {
        if let Some(existing) = meta_mgr.get_fs_type_by_name(desc.name()).await? {
            fs_id_map.insert(desc.fs_id(), existing.fs_id());
            continue;
        }
        let new_id = meta_mgr.create_fs_type(desc.name(), desc.kind()).await?;
        fs_id_map.insert(desc.fs_id(), new_id);
    }
    Ok(fs_id_map)
}

/// Creates the dump's tables on the target server inside one DDL transaction
/// (system tables are not in the dump; the live server has them).
/// `drop_existing` drops and recreates a same-named table; otherwise an
/// already-present table is skipped.
async fn restore_tables(
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
    schemas: &[SchemaTable],
    fs_id_map: &HashMap<u64, u64>,
    drop_existing: bool,
) -> RS<RestoredTables> {
    let mut created = 0u64;
    let mut skipped = 0u64;
    let mut names: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let ddl_tx = x_contract.begin_tx().await?;
    for schema in schemas {
        if meta_mgr
            .get_table_exact(schema.schema(), schema.table_name())
            .await?
            .is_some()
        {
            if drop_existing {
                let existing = meta_mgr
                    .get_table_exact(schema.schema(), schema.table_name())
                    .await?
                    .ok_or_else(|| {
                        mudu_error!(ErrorCode::Internal, "table disappeared during restore")
                    })?;
                x_contract.drop_table(ddl_tx.clone(), existing.id()).await?;
            } else {
                skipped += 1;
                continue;
            }
        }
        let rebuilt = remap_fs_bindings(schema, fs_id_map);
        x_contract.create_table(ddl_tx.clone(), &rebuilt).await?;
        names.insert((schema.schema().clone(), schema.table_name().clone()));
        created += 1;
    }
    x_contract.commit_tx(ddl_tx).await?;
    Ok(RestoredTables {
        created,
        skipped,
        names,
    })
}

/// Restores the partition rules, bindings and placements of a dump. Rule and
/// table OIDs are preserved; placements are re-mapped from the source worker
/// index to the target worker id.
async fn restore_partitions(
    meta_mgr: &Arc<dyn MetaMgr>,
    workers: &[(Arc<WorkerRuntime>, u32)],
    manifest: &LogicalDumpManifest,
) -> RS<()> {
    // Partition rules (ids are preserved OIDs; skip an existing same-name rule).
    for rule in &manifest.partition_rules {
        if meta_mgr
            .get_partition_rule_by_name(&rule.name)
            .await?
            .is_none()
        {
            meta_mgr.create_partition_rule(rule).await?;
        }
    }

    // Partition bindings (reference the preserved table/rule OIDs).
    for binding in &manifest.partition_bindings {
        if meta_mgr
            .get_table_partition_binding(binding.table_id)
            .await?
            .is_none()
        {
            meta_mgr.bind_table_partition(binding).await?;
        }
    }

    // Partition placements: re-map the source worker index to the target
    // worker id.
    let target_worker_id = |worker_index: u32| -> RS<OID> {
        workers
            .iter()
            .find(|(_, index)| *index == worker_index)
            .map(|(worker, _)| worker.worker_id())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no worker at index {worker_index} on the target server")
                )
            })
    };
    let mut placements = Vec::with_capacity(manifest.placements.len());
    for placement in &manifest.placements {
        placements.push(PartitionPlacement {
            partition_id: placement.partition_id,
            worker_id: target_worker_id(placement.worker_index)?,
        });
    }
    if !placements.is_empty() {
        meta_mgr.upsert_partition_placements(&placements).await?;
    }
    Ok(())
}

/// Reloads the CSV data of every table created during this restore (tables
/// skipped as already-present are left untouched); returns the total rows
/// loaded.
async fn load_table_data(
    dir: &str,
    label: &str,
    manifest: &LogicalDumpManifest,
    schemas: &[SchemaTable],
    created_tables: &std::collections::HashSet<(String, String)>,
    meta_mgr: &Arc<dyn MetaMgr>,
    x_contract: &Arc<dyn XContract>,
) -> RS<u64> {
    let async_runtime = default_sys_io_context().provider_arc();
    let mut rows_loaded = 0u64;
    for table in &manifest.tables {
        if !created_tables.contains(&(table.schema.clone(), table.name.clone())) {
            continue;
        }
        let table_ref = schemas
            .iter()
            .find(|schema| schema.schema() == &table.schema && schema.table_name() == &table.name)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::CorruptedData,
                    format!(
                        "dump references missing table {}.{}",
                        table.schema, table.name
                    )
                )
            })?;
        let csv_path = table_csv_path(dir, label, &table.schema, &table.name);
        if !mudu_sys::fs::sync::sync_path_exists(&csv_path) {
            warn!(
                table = %table.name,
                "logical restore: no data CSV (metadata-only table)"
            );
            continue;
        }
        let table_desc: Arc<TableDesc> = meta_mgr.get_table_by_id(table_ref.id()).await?;
        let layout = CopyLayout::new(&table_desc, &[])?;
        let tx_mgr = x_contract.begin_tx().await?;
        let cmd = LoadFromFile::new(LoadFromFileParams {
            csv_file: csv_path.to_string_lossy().to_string(),
            tx_mgr: tx_mgr.clone(),
            table_id: table_desc.id(),
            key_index: layout.key_index().to_vec(),
            value_index: layout.value_index().to_vec(),
            x_contract: x_contract.clone(),
            meta_mgr: meta_mgr.clone(),
            async_runtime: Some(async_runtime.clone()),
        });
        cmd.prepare().await?;
        cmd.run().await?;
        let rows = cmd.affected_rows().await?;
        x_contract.commit_tx(tx_mgr).await?;
        rows_loaded += rows;
    }
    Ok(rows_loaded)
}

/// Rebuilds a `SchemaTable` with its filesystem-type column bindings re-mapped
/// to the target server's fs ids.
fn remap_fs_bindings(schema: &SchemaTable, fs_id_map: &HashMap<u64, u64>) -> SchemaTable {
    let mut columns = schema.columns().clone();
    for column in &mut columns {
        if let Some(binding) = column.fs_binding() {
            if let Some(new_id) = fs_id_map.get(&binding.fs_id()) {
                column.set_fs_binding(Some(FsColumnBinding::new(*new_id, binding.kind())));
            }
        }
    }
    let mut rebuilt = SchemaTable::new_with_oid(
        schema.id(),
        schema.table_name().clone(),
        columns,
        schema.key_indices().clone(),
        schema.value_indices().clone(),
    );
    rebuilt.set_schema(schema.schema().clone());
    rebuilt
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use ::catalog::contract::schema_column::SchemaColumn;
    use mudu_type::data_type_info::DataTypeInfo;

    fn sample_schema() -> SchemaTable {
        let mut id = SchemaColumn::new(
            "id".to_string(),
            TypeFamily::I32,
            DataTypeInfo::from_text(TypeFamily::I32, String::new()),
        );
        id.set_primary_index(Some(0));
        id.set_nullable(false);
        let mut amount = SchemaColumn::new(
            "amount".to_string(),
            TypeFamily::Numeric,
            DataTypeInfo::from_text(
                TypeFamily::Numeric,
                "{\"precision\":9,\"scale\":4}".to_string(),
            ),
        );
        amount.set_primary_index(None);
        let note = SchemaColumn::new(
            "note".to_string(),
            TypeFamily::String,
            DataTypeInfo::from_text(TypeFamily::String, "{\"length\":16}".to_string()),
        );
        SchemaTable::new(
            "ledger".to_string(),
            vec![id, amount, note],
            vec![0],
            vec![1, 2],
        )
    }

    #[test]
    fn sql_column_type_renders_numeric_and_varchar() {
        let schema = sample_schema();
        assert_eq!(sql_column_type(&schema.columns()[0], &[]), "INT");
        assert_eq!(sql_column_type(&schema.columns()[1], &[]), "NUMERIC(9, 4)");
        assert_eq!(sql_column_type(&schema.columns()[2], &[]), "VARCHAR(16)");
    }

    #[test]
    fn render_create_table_ddl_contains_primary_key_and_types() {
        let schema = sample_schema();
        let ddl = render_create_table_ddl(&schema, &[]);
        assert!(ddl.contains("CREATE TABLE mududb.ledger"));
        assert!(ddl.contains("id INT NOT NULL"));
        assert!(ddl.contains("NUMERIC(9, 4)"));
        assert!(ddl.contains("VARCHAR(16)"));
        assert!(ddl.contains("PRIMARY KEY (id)"));
    }

    #[test]
    fn table_csv_file_uses_schema_dot_name() {
        assert_eq!(table_csv_file("demo", "t"), "demo.t.csv");
    }
}
