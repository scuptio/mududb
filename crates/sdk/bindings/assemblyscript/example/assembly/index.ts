import {
  Database,
  FS_O_RDONLY,
  FS_O_WRONLY,
  SqlStmt,
  Value,
  ValueList,
  fsClose,
  fsOpen,
  fsRead,
  fsWrite,
} from "../../assembly";

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

export function fsSmoke(oidHi: u64, oidLo: u64): void {
  const db = Database.open("");
  const session = db.id;

  const data = String.UTF8.encode("hello fs", false);

  const writeFd = fsOpen(session.hi, session.lo, oidHi, oidLo, "hello.txt", FS_O_WRONLY).unwrap();
  fsWrite(session.hi, session.lo, writeFd, data).unwrap();
  fsClose(session.hi, session.lo, writeFd).unwrap();

  const readFd = fsOpen(session.hi, session.lo, oidHi, oidLo, "hello.txt", FS_O_RDONLY).unwrap();
  fsRead(session.hi, session.lo, readFd, <u32>data.byteLength).unwrap();
  fsClose(session.hi, session.lo, readFd).unwrap();

  db.close();
}
