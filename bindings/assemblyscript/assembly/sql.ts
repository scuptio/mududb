import {
  ResourceHandle,
  Value,
  witSqlStmtNew,
  witValueListBindNamedValue,
  witValueListBindValue,
  witValueListLen,
  witValueListNew,
  witValueListValue,
} from "./wit";

export class ValueList {
  readonly raw: ResourceHandle;

  constructor(raw: ResourceHandle = 0) {
    this.raw = raw == 0 ? witValueListNew() : raw;
  }

  static fromRaw(raw: ResourceHandle): ValueList {
    return new ValueList(raw);
  }

  bind(index: i32, value: Value): ValueList {
    witValueListBindValue(this.raw, index, value);
    return this;
  }

  bindNamed(name: string, value: Value): ValueList {
    witValueListBindNamedValue(this.raw, name, value);
    return this;
  }

  len(): u32 {
    return witValueListLen(this.raw);
  }

  value(index: u32): Value {
    return witValueListValue(this.raw, index).unwrap();
  }
}

export class SqlStmt {
  readonly raw: ResourceHandle;

  constructor(sql: string) {
    this.raw = witSqlStmtNew(sql);
  }
}
