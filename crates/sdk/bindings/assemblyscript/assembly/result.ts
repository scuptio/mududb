import { UniDataValue } from "./generated/UniDataValue";
import { UniQueryResult } from "./generated/UniQueryResult";
import { UniTupleRow } from "./generated/UniTupleRow";
import { MuduError, Value } from "./wit";
import { ValueList } from "./sql";

export class Result<T> {
  private ok_: bool;
  private value_: T;
  private error_: MuduError;

  private constructor(ok: bool, value: T, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  static ok<T>(value: T): Result<T> {
    return new Result<T>(true, value);
  }

  static err<T>(message: string): Result<T> {
    return new Result<T>(false, defaultResultValue<T>(), new MuduError(1, message, "assemblyscript", ""));
  }

  static error<T>(error: MuduError): Result<T> {
    return new Result<T>(false, defaultResultValue<T>(), error);
  }

  get isOk(): bool {
    return this.ok_;
  }

  get isErr(): bool {
    return !this.ok_;
  }

  unwrap(): T {
    if (!this.ok_) {
      throw new Error(this.error_.message);
    }
    return this.value_;
  }

  unwrapErr(): MuduError {
    return this.error_;
  }
}

function defaultResultValue<T>(): T {
  if (isReference<T>()) {
    return changetype<T>(0);
  }
  return <T>0;
}

export function procedureResultOk(values: ValueList): Result<ValueList> {
  return Result.ok<ValueList>(values);
}

export function procedureResultErr(
  error: MuduError,
  procedure: string = "",
  location: string = "",
): Result<ValueList> {
  const source = error.source.length > 0 ? error.source : "assemblyscript";
  const errorLocation = location.length > 0 ? location : procedure;
  return Result.error<ValueList>(
    new MuduError(error.code, error.message, source, errorLocation),
  );
}

// Pure in-memory result set: the host drains all rows into the first query
// response (`eof = true`), so iteration never goes back to the wire. The
// cursor starts before the first row, `next()` advances it, and
// `currentRow()` is valid only after a successful `next()`.
export class ResultSet {
  private columns: Array<string>;
  private rows: Array<UniTupleRow>;
  private cursor: i32;

  constructor(result: UniQueryResult) {
    this.rows = result.result_set.row_set;
    this.cursor = 0;
    const fields = result.tuple_desc.record_fields;
    const columns = new Array<string>(fields.length);
    for (let i = 0; i < fields.length; i++) {
      columns[i] = fields[i].field_name;
    }
    this.columns = columns;
  }

  next(): bool {
    if (this.cursor < this.rows.length) {
      this.cursor += 1;
      return true;
    }
    return false;
  }

  currentRow(): Row {
    if (this.cursor == 0 || this.cursor > this.rows.length) {
      throw new Error("no current row");
    }
    return new Row(this.columns, this.rows[this.cursor - 1].fields);
  }

  columnCount(): u32 {
    return this.columns.length as u32;
  }

  columnName(column: u32): string {
    if (column >= <u32>this.columns.length) {
      throw new Error("column index is out of range");
    }
    return this.columns[column];
  }

  findColumn(name: string): u32 {
    for (let i = 0; i < this.columns.length; i++) {
      if (this.columns[i] == name) {
        return i as u32;
      }
    }
    throw new Error("column name not found: " + name);
  }

  eof(): bool {
    return this.cursor >= this.rows.length;
  }
}

export class Row {
  private columns: Array<string>;
  private values: Array<UniDataValue>;

  constructor(columns: Array<string>, values: Array<UniDataValue>) {
    this.columns = columns;
    this.values = values;
  }

  isNull(column: u32): bool {
    return this.value(column).isNull();
  }

  isNullByName(name: string): bool {
    return this.isNull(this.findColumn(name));
  }

  value(column: u32): Value {
    if (column >= <u32>this.values.length) {
      throw new Error("column index is out of range");
    }
    return Value.fromUniDataValue(this.values[column]);
  }

  valueByName(name: string): Value {
    return this.value(this.findColumn(name));
  }

  private findColumn(name: string): u32 {
    for (let i = 0; i < this.columns.length; i++) {
      if (this.columns[i] == name) {
        return i as u32;
      }
    }
    throw new Error("column name not found: " + name);
  }
}
