# Page Size Migration Tool — TODO

## Background

`page_size` has been moved from a hard-coded kernel constant into the server
configuration (`MuduDBCfg::page_size` and `ServerCfg::page_size`). It is
classified as a **persistent** config: it is written into the on-disk format of
page files and must not be changed for an existing database directory without a
migration.

## TODO

Implement a migration tool / command that:

1. Reads the current on-disk page size from a reliable source (page file
   headers or a database metadata file).
2. Reads the target page size from the new configuration.
3. Verifies the target page size is a power of two and at least 4096.
4. For every page file in the data directory:
   - Read each old page.
   - Migrate the page header / slots / tailer to the new page size.
   - Rewrite the file with the new page layout.
5. Updates the persisted database metadata to record the new page size.
6. Validates the migrated files (checksums, page count, header magic).
7. Provides a dry-run mode and a backup/rollback strategy.

## Notes

- The kernel storage layer currently still uses the compile-time
  `PAGE_SIZE` alias in many places. Before the migration tool can be useful,
  the storage layer must be refactored to accept a runtime page size from
  `ServerCfg::page_size()`.
- Changing `page_size` affects file offsets, page counts, cache sizes, and WAL
  layout, so the migration must be coordinated across all storage modules.
