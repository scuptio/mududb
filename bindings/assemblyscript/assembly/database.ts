import { ResultSet } from "./result";
import { SqlStmt, ValueList } from "./sql";
import { Oid, witBatch, witClose, witCommand, witOpen, witQuery } from "./wit";

export class Database {
  readonly id: Oid;

  private constructor(id: Oid) {
    this.id = id;
  }

  static open(uri: string = ""): Database {
    return new Database(witOpen(uri).unwrap());
  }

  close(): void {
    witClose(this.id).unwrap();
  }

  query(stmt: SqlStmt, values: ValueList = new ValueList()): ResultSet {
    return new ResultSet(witQuery(this.id, stmt.raw, values.raw).unwrap());
  }

  command(stmt: SqlStmt, values: ValueList = new ValueList()): u64 {
    return witCommand(this.id, stmt.raw, values.raw).unwrap();
  }

  batch(stmt: SqlStmt, values: ValueList = new ValueList()): u64 {
    return witBatch(this.id, stmt.raw, values.raw).unwrap();
  }
}
