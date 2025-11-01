use libsql::{Database, Connection};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub index: u64,
    pub name: String,
    pub type_name: String,
    pub origin_table: Option<String>,
    pub origin_column: Option<String>,
}

/// Get schema information for a SQL query result set
/// This function executes the query with LIMIT 0 to get only the structure without data
pub async fn get_query_schema(
    conn: &Connection,
    query: &str,
) -> Result<Vec<ColumnInfo>, Box<dyn std::error::Error>> {
    // Use LIMIT 0 to get only structure without data
    let limited_query = if !query.to_lowercase().trim().starts_with("explain") {
        format!("SELECT * FROM ({}) LIMIT 0", query)
    } else {
        query.to_string()
    };

    let mut stmt = conn.prepare(&limited_query).await?;
    let column_count = stmt.column_count();

    let mut schema = Vec::with_capacity(column_count);
    let columns = stmt.columns();
    for i in 0..column_count {
        let column = &columns[i];
        let name = column.name().to_string();

        let decl_type = column.decl_type().map(|s| s.to_string());

        // Get column origin information
        let origin_table = column.origin_name().map(|s| s.to_string());
        let origin_column = column.table_name().map(|s| s.to_string());

        schema.push(ColumnInfo {
            index: i as _,
            name,
            type_name: decl_type.unwrap_or_else(|| "unknown".to_string()),
            origin_table,
            origin_column,
        });
    }

    Ok(schema)
}


/// Generate a type mapping report for a query
pub async fn generate_type_report(
    conn: &Connection,
    query: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let schema = get_query_schema(conn, query).await?;
    let mut type_report = HashMap::new();

    for col in schema {
        type_report.insert(col.name, col.type_name);
    }

    Ok(type_report)
}

/// Compare the structure of two queries to check if they have the same schema
pub async fn compare_query_schemas(
    conn: &Connection,
    query1: &str,
    query2: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let schema1 = get_query_schema(conn, query1).await?;
    let schema2 = get_query_schema(conn, query2).await?;

    if schema1.len() != schema2.len() {
        return Ok(false);
    }

    for (col1, col2) in schema1.iter().zip(schema2.iter()) {
        if col1.name != col2.name || col1.type_name != col2.type_name {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Print schema information in a formatted way
pub async fn print_query_schema(
    conn: &Connection,
    query: &str,
    title: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = get_query_schema(conn, query).await?;

    println!("\n=== {} ===", title);
    println!("SQL: {}", query);
    println!("Result Set Schema:");
    println!("{:<20} | {:<15} | {:<10} | {:<10} | {:<10}",
             "Column Name", "Declared Type", "Value Type", "Origin Table", "Origin Column");
    println!("{}", "-".repeat(80));

    for col in schema {
        println!("{:<20} | {:<15} | {:<10} | {:<10}",
                 col.name,
                 col.type_name,
                 col.origin_table.unwrap_or_default(),
                 col.origin_column.unwrap_or_default()
        );
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory database
    let db = Database::open(":memory:")?;
    let conn = db.connect()?;

    // Create test table
    conn.execute(
        "CREATE TABLE employees (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            salary REAL,
            department TEXT,
            hire_date TEXT,
            is_active INTEGER
        )",
        (),
    ).await?;

    // Insert test data
    conn.execute(
        "INSERT INTO employees (name, salary, department, hire_date, is_active)
         VALUES (?, ?, ?, ?, ?)",
        ("John Doe", 50000.0, "Engineering", "2020-01-15", 1),
    ).await?;

    // Create another table for JOIN testing
    conn.execute(
        "CREATE TABLE departments (
            dept_id INTEGER PRIMARY KEY,
            dept_name TEXT NOT NULL,
            budget REAL
        )",
        (),
    ).await?;

    conn.execute(
        "INSERT INTO departments (dept_name, budget) VALUES (?, ?)",
        ("Engineering", 1000000.0),
    ).await?;

    // Test different types of queries
    let test_queries = vec![
        ("SELECT * FROM employees", "Simple SELECT *"),
        ("SELECT id, name, salary FROM employees WHERE salary > 40000", "Filtered SELECT"),
        ("SELECT e.name, e.department, e.salary FROM employees e", "Aliased table"),
        ("SELECT name as employee_name, salary * 1.1 as increased_salary FROM employees", "Calculated columns"),
        ("SELECT e.name, d.dept_name, d.budget FROM employees e JOIN departments d ON e.department = d.dept_name", "JOIN query"),
        ("SELECT department, COUNT(*) as employee_count FROM employees GROUP BY department", "GROUP BY query"),
    ];

    for (i, (query, description)) in test_queries.iter().enumerate() {
        print_query_schema(&conn, query, &format!("Test Query {}: {}", i + 1, description)).await?;
    }

    // Generate type report
    println!("\n=== Type Mapping Report ===");
    let report = generate_type_report(&conn, "SELECT * FROM employees").await?;
    for (col_name, col_type) in report {
        println!("  {}: {}", col_name, col_type);
    }

    // Compare query schemas
    println!("\n=== Schema Comparison ===");
    let same1 = compare_query_schemas(
        &conn,
        "SELECT id, name FROM employees",
        "SELECT name, id FROM employees"
    ).await?;
    println!("Queries have same structure: {}", same1);

    let same2 = compare_query_schemas(
        &conn,
        "SELECT id, name FROM employees",
        "SELECT id, name, salary FROM employees"
    ).await?;
    println!("Queries have same structure: {}", same2);

    // Display detailed schema information
    println!("\n=== Detailed Schema Information ===");
    let detailed_schema = get_query_schema(&conn, "SELECT * FROM employees").await?;

    for col in detailed_schema {
        println!("Index: {}", col.index);
        println!("  Column Name: {}", col.name);
        println!("  Declared Type: {:?}", col.type_name);
        println!("  Origin Table: {:?}", col.origin_table);
        println!("  Origin Column: {:?}", col.origin_column);
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_schema() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open(":memory:")?;
        let conn = db.connect()?;

        conn.execute(
            "CREATE TABLE test (id INTEGER, name TEXT, value REAL)",
            (),
        ).await?;

        let schema = get_query_schema(&conn, "SELECT * FROM test").await?;

        assert_eq!(schema.len(), 3);
        assert_eq!(schema[0].name, "id");
        assert_eq!(schema[1].name, "name");
        assert_eq!(schema[2].name, "value");

        Ok(())
    }

    #[tokio::test]
    async fn test_complex_query_schema() -> Result<(), Box<dyn std::error::Error>> {
        let db = Database::open(":memory:")?;
        let conn = db.connect()?;

        conn.execute(
            "CREATE TABLE users (user_id INTEGER, username TEXT)",
            (),
        ).await?;

        conn.execute(
            "CREATE TABLE orders (order_id INTEGER, user_id INTEGER, amount REAL)",
            (),
        ).await?;

        let schema = get_query_schema(
            &conn,
            "SELECT u.username, o.order_id, o.amount FROM users u JOIN orders o ON u.user_id = o.user_id"
        ).await?;

        assert_eq!(schema.len(), 3);
        assert_eq!(schema[0].name, "username");
        assert_eq!(schema[1].name, "order_id");
        assert_eq!(schema[2].name, "amount");

        Ok(())
    }
}