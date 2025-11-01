## Development Requirements:

### ER Diagram:
    Provide an Entity-Relationship diagram using PlantUML.

### DDL Specifications:

Use SQLite syntax with the following limitations:

1. No support for foreign keys, indexes, or timestamp types.

2. No auto-increment key support.

Replacements:

1. Use i64 to represent timestamps.

2. Use UUID strings in place of auto-increment keys.

3. For atomic timestamp fields, use integer numeric types.

## Rust Implementation:
    
Provide Mudu procedures in the Rust programming language.

### Show all the source code