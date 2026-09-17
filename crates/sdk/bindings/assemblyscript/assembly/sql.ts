import { UniDataValue } from "./generated/UniDataValue";
import { UniSqlParam } from "./generated/UniSqlParam";
import { Value } from "./wit";

export class ValueList {
  private items: Array<Value>;
  private names: Array<string> | null;

  constructor() {
    this.items = new Array<Value>(0);
    this.names = null;
  }

  /**
   * Bind `value` to the positional `?` placeholder `index`.
   *
   * Indices must be contiguous from 0 (bind 0..n-1 for n placeholders);
   * gaps, negative indices, and re-binding the same index are rejected.
   */
  bind(index: i32, value: Value): ValueList {
    if (this.names !== null) {
      throw new Error("positional bind cannot be mixed with bindNamed");
    }
    if (index != this.items.length) {
      throw new Error("bind indices must be contiguous from 0");
    }
    this.items.push(value);
    return this;
  }

  /**
   * Bind `value` to the named placeholder `:name` in the SQL statement.
   *
   * Named parameters are resolved host-side: the statement text keeps the
   * `:name` placeholders, and the host rewrites them to positional form,
   * looking each name up in this list (a repeated `:name` reuses its
   * value). Indexed (`bind`) and named (`bindNamed`) values cannot be
   * mixed in one list.
   */
  bindNamed(name: string, value: Value): ValueList {
    if (this.names === null) {
      if (this.items.length > 0) {
        throw new Error("bindNamed cannot be mixed with positional bind");
      }
      this.names = new Array<string>(0);
    }
    this.items.push(value);
    this.names!.push(name);
    return this;
  }

  len(): u32 {
    return this.items.length as u32;
  }

  value(index: u32): Value {
    return this.items[index];
  }

  // Wire form for query/command/batch argv: positional values under the
  // `params` key; the `param-names` key is present only for named binding.
  toUniSqlParam(): UniSqlParam {
    const param = new UniSqlParam();
    param.params = this.toUniDataValues();
    param.param_names = this.names;
    return param;
  }

  toUniDataValues(): Array<UniDataValue> {
    const out = new Array<UniDataValue>(this.items.length);
    for (let i = 0; i < this.items.length; i++) {
      out[i] = this.items[i].toUniDataValue();
    }
    return out;
  }

  static fromUniDataValues(values: Array<UniDataValue>): ValueList {
    const list = new ValueList();
    for (let i = 0; i < values.length; i++) {
      list.items.push(Value.fromUniDataValue(values[i]));
    }
    return list;
  }
}

export class SqlStmt {
  readonly sql: string;

  constructor(sql: string) {
    this.sql = sql;
  }
}
