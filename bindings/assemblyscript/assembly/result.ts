import {
  MuduError,
  ResourceHandle,
  Value,
  witResultSetColumnCount,
  witResultSetColumnName,
  witResultSetCurrentRow,
  witResultSetEof,
  witResultSetFindColumn,
  witResultSetNext,
  witRowIsNull,
  witRowIsNullByName,
  witRowValue,
  witRowValueByName,
} from "./wit";
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
    return new Result<T>(false, changetype<T>(0), new MuduError(1, message, "assemblyscript", ""));
  }

  static error<T>(error: MuduError): Result<T> {
    return new Result<T>(false, changetype<T>(0), error);
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

export class ResultSet {
  readonly raw: ResourceHandle;

  constructor(raw: ResourceHandle) {
    this.raw = raw;
  }

  next(): bool {
    return witResultSetNext(this.raw).unwrap();
  }

  currentRow(): Row {
    return new Row(witResultSetCurrentRow(this.raw).unwrap());
  }

  columnCount(): u32 {
    return witResultSetColumnCount(this.raw).unwrap();
  }

  columnName(column: u32): string {
    return witResultSetColumnName(this.raw, column).unwrap();
  }

  findColumn(name: string): u32 {
    return witResultSetFindColumn(this.raw, name).unwrap();
  }

  eof(): bool {
    return witResultSetEof(this.raw).unwrap();
  }
}

export class Row {
  readonly raw: ResourceHandle;

  constructor(raw: ResourceHandle) {
    this.raw = raw;
  }

  isNull(column: u32): bool {
    return witRowIsNull(this.raw, column).unwrap();
  }

  isNullByName(name: string): bool {
    return witRowIsNullByName(this.raw, name).unwrap();
  }

  value(column: u32): Value {
    return witRowValue(this.raw, column).unwrap();
  }

  valueByName(name: string): Value {
    return witRowValueByName(this.raw, name).unwrap();
  }
}
