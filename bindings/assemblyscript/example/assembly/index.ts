import { Database, SqlStmt, Value, ValueList } from "../../assembly";

export function smoke(): void {
  const db = Database.open("");

  db.command(new SqlStmt("create table if not exists kv (k text primary key, v text)"));

  const insertParams = new ValueList();
  insertParams.bindNamed("k", Value.text("hello"));
  insertParams.bindNamed("v", Value.text("world"));
  db.command(new SqlStmt("insert into kv(k, v) values(:k, :v)"), insertParams);

  const queryParams = new ValueList();
  queryParams.bindNamed("k", Value.text("hello"));
  const rows = db.query(new SqlStmt("select v from kv where k = :k"), queryParams);

  while (rows.next()) {
    const row = rows.currentRow();
    row.valueByName("v").asText();
  }

  db.close();
}
