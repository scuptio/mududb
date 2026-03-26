#[cfg(test)]
mod _test {
    use crate::ast::parser::SQLParser;
    use mudu::common::result::RS;

    use project_root::get_project_root;
    use std::fs;
    use std::path::Path;

    fn parse_sql(sql: &String) -> RS<()> {
        let parser = SQLParser::new();
        let stmt_list = parser.parse(sql)?;
        println!("stmt: {:?}", stmt_list);
        Ok(())
    }

    #[test]
    fn test_select() {
        let sql = "
        select
            distinct column1,
            column2
        from table1
        where column3 = 1;"
            .to_string();
        let r = parse_sql(&sql);
        assert!(r.is_ok());

        let sql2 = "
        select
            distinct column1,
            column2
        from table1
        where column3 = 1"
            .to_string();
        let r = parse_sql(&sql2);
        assert!(r.is_ok());
    }

    #[test]
    fn test_update() {
        let sql = "\
UPDATE Customers \
SET ContactName = 'Alfred Schmidt', City= 'Frankfurt' \
WHERE CustomerID = 1;"
            .to_string();
        let r = parse_sql(&sql);
        if r.is_err() {
            println!("{:#?}", r);
        }
        assert!(r.is_ok());
    }

    #[test]
    fn test_delete() {
        let sql = " DELETE FROM Customers
    WHERE CustomerName='Alfreds Futterkiste'; "
            .to_string();
        let r = parse_sql(&sql);
        assert!(r.is_ok());
    }

    #[test]
    fn test_insert() {
        let sql = "
    INSERT INTO Customers (
        CustomerName,
        ContactName,
        Address,
        City,
        PostalCode,
        Country
    )
    VALUES (
        'Cardinal',
        'Tom B. Erichsen',
        'Skagen 21',
        'Stavanger',
        '4006',
        'Norway'
    );
    INSERT INTO Customers (
        CustomerName,
        ContactName,
        Address,
        City,
        PostalCode,
        Country
    )
    VALUES (
        'Cardinal',
        'Tom B. Erichsen',
        'Skagen 21',
        'Stavanger',
        '4006',
        'Norway'
    );
    "
        .to_string();
        let r = parse_sql(&sql);
        assert!(r.is_ok());
    }

    #[test]
    fn test_create_table() {
        let sql = "
    CREATE TABLE Persons (
        PersonID int PRIMARY KEY,
        LastName char(255),
        FirstName char(255),
        Address char(255),
        City char(255)
    );"
        .to_string();
        let r = parse_sql(&sql);
        assert!(r.is_ok());

        let sql = "
    CREATE TABLE CUSTOMERS(
           ID1          INT,
           ID2          INT,
           NAME        CHAR (20),
           AGE         INT,
           ADDRESS     CHAR (25),
           SALARY      INT,
           PRIMARY KEY (ID1, ID2)
    );"
        .to_string();
        let r = parse_sql(&sql);
        assert!(r.is_ok());
    }

    fn parse_file<P: AsRef<Path>>(path: P) -> RS<()> {
        let sql = fs::read_to_string(path).unwrap();
        parse_sql(&sql)?;
        Ok(())
    }

    #[test]
    fn test_parse_ddl_file() {
        let path = get_project_root().unwrap();
        let path = if path.file_name().unwrap().to_str().unwrap().eq("sql_parser") {
            path
        } else {
            path.join("sql_parser")
        };
        println!("path: {:?}", path);
        let path = path.join("data/ddl.sql");
        let r = parse_file(path);
        println!("{:?}", r);
        assert!(r.is_ok())
    }
}
