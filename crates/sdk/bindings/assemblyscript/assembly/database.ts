import { ResultSet } from "./result";
import { SqlStmt, ValueList } from "./sql";
import { Oid } from "./wit";
import { witBatch, witClose, witCommand, witOpen, witQuery } from "./syscall";

export class Database {
  readonly id: Oid;

  private constructor(id: Oid) {
    this.id = id;
  }

  static open(uri: string = ""): Database {
    return new Database(witOpen(uri));
  }

  close(): void {
    witClose(this.id);
  }

  query(stmt: SqlStmt, values: ValueList = new ValueList()): ResultSet {
    return witQuery(this.id, stmt, values);
  }

  command(stmt: SqlStmt, values: ValueList = new ValueList()): u64 {
    return witCommand(this.id, stmt, values);
  }

  batch(stmt: SqlStmt, values: ValueList = new ValueList()): u64 {
    return witBatch(this.id, stmt, values);
  }
}
