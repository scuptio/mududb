export type ResourceHandle = u32;

const VALUE_RESULT_VALUE_OFFSET: usize = 8;
const VALUE_PAYLOAD_OFFSET: usize = 8;
const ERROR_RESULT_VALUE_OFFSET_4: usize = 4;
const ERROR_RESULT_VALUE_OFFSET_8: usize = 8;
const ERROR_SIZE: usize = 28;
const VALUE_SIZE: usize = 24;
const RESULT_VALUE_SIZE: usize = 40;
const RESULT_ERROR_SIZE: usize = 36;

export class Oid {
  hi: u64;
  lo: u64;

  constructor(hi: u64 = 0, lo: u64 = 0) {
    this.hi = hi;
    this.lo = lo;
  }
}

export class MuduError {
  code: u32;
  message: string;
  source: string;
  location: string;

  constructor(code: u32 = 0, message: string = "", source: string = "", location: string = "") {
    this.code = code;
    this.message = message;
    this.source = source;
    this.location = location;
  }
}

export enum ValueKind {
  Null,
  Boolean,
  Int64,
  Float64,
  Text,
  Binary,
  ObjectId,
}

export class Value {
  kind: ValueKind;
  boolValue: bool;
  int64Value: i64;
  float64Value: f64;
  textValue: string;
  binaryValue: Uint8Array;
  oidValue: Oid;

  constructor(kind: ValueKind = ValueKind.Null) {
    this.kind = kind;
    this.boolValue = false;
    this.int64Value = 0;
    this.float64Value = 0.0;
    this.textValue = "";
    this.binaryValue = new Uint8Array(0);
    this.oidValue = new Oid();
  }

  static null(): Value {
    return witValueNull();
  }

  static boolean(input: bool): Value {
    return witValueFromBoolean(input);
  }

  static int64(input: i64): Value {
    return witValueFromInt64(input);
  }

  static float64(input: f64): Value {
    return witValueFromFloat64(input);
  }

  static text(input: string): Value {
    return witValueFromText(input);
  }

  static binary(input: Uint8Array): Value {
    return witValueFromBinary(input);
  }

  static objectId(input: Oid): Value {
    return witValueFromOid(input);
  }

  isNull(): bool {
    return witValueIsNull(this);
  }

  asBoolean(): bool {
    return witValueAsBoolean(this).unwrap();
  }

  asInt64(): i64 {
    return witValueAsInt64(this).unwrap();
  }

  asFloat64(): f64 {
    return witValueAsFloat64(this).unwrap();
  }

  asText(): string {
    return witValueAsText(this).unwrap();
  }

  asBinary(): Uint8Array {
    return witValueAsBinary(this).unwrap();
  }

  asObjectId(): Oid {
    return witValueAsOid(this).unwrap();
  }
}

export class VoidResult {
  private ok_: bool;
  private error_: MuduError;

  constructor(ok: bool, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.error_ = error;
  }

  get isOk(): bool {
    return this.ok_;
  }

  get isErr(): bool {
    return !this.ok_;
  }

  unwrap(): void {
    if (!this.ok_) {
      throw new Error(this.error_.message);
    }
  }

  unwrapErr(): MuduError {
    return this.error_;
  }
}

export class BoolResult {
  private ok_: bool;
  private value_: bool;
  private error_: MuduError;

  constructor(ok: bool, value: bool = false, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): bool {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class U32Result {
  private ok_: bool;
  private value_: u32;
  private error_: MuduError;

  constructor(ok: bool, value: u32 = 0, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): u32 {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class U64Result {
  private ok_: bool;
  private value_: u64;
  private error_: MuduError;

  constructor(ok: bool, value: u64 = 0, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): u64 {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class I64Result {
  private ok_: bool;
  private value_: i64;
  private error_: MuduError;

  constructor(ok: bool, value: i64 = 0, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): i64 {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class F64Result {
  private ok_: bool;
  private value_: f64;
  private error_: MuduError;

  constructor(ok: bool, value: f64 = 0, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): f64 {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class StringResult {
  private ok_: bool;
  private value_: string;
  private error_: MuduError;

  constructor(ok: bool, value: string = "", error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): string {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class BytesResult {
  private ok_: bool;
  private value_: Uint8Array;
  private error_: MuduError;

  constructor(ok: bool, value: Uint8Array = new Uint8Array(0), error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): Uint8Array {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class OidResult {
  private ok_: bool;
  private value_: Oid;
  private error_: MuduError;

  constructor(ok: bool, value: Oid = new Oid(), error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): Oid {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class ValueResult {
  private ok_: bool;
  private value_: Value;
  private error_: MuduError;

  constructor(ok: bool, value: Value = new Value(), error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): Value {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

export class ResourceResult {
  private ok_: bool;
  private value_: ResourceHandle;
  private error_: MuduError;

  constructor(ok: bool, value: ResourceHandle = 0, error: MuduError = new MuduError()) {
    this.ok_ = ok;
    this.value_ = value;
    this.error_ = error;
  }

  unwrap(): ResourceHandle {
    if (!this.ok_) throw new Error(this.error_.message);
    return this.value_;
  }
}

@external("mududb:component-shim/types", "value-null")
declare function rawValueNull(result: usize): void;
@external("mududb:component-shim/types", "value-from-boolean")
declare function rawValueFromBoolean(input: bool, result: usize): void;
@external("mududb:component-shim/types", "value-from-int64")
declare function rawValueFromInt64(input: i64, result: usize): void;
@external("mududb:component-shim/types", "value-from-float64")
declare function rawValueFromFloat64(input: f64, result: usize): void;
@external("mududb:component-shim/types", "value-from-text")
declare function rawValueFromText(ptr: usize, len: usize, result: usize): void;
@external("mududb:component-shim/types", "value-from-binary")
declare function rawValueFromBinary(ptr: usize, len: usize, result: usize): void;
@external("mududb:component-shim/types", "value-from-oid")
declare function rawValueFromOid(hi: u64, lo: u64, result: usize): void;
@external("mududb:component-shim/types", "value-is-null")
declare function rawValueIsNull(tag: i32, payload1: i64, payload2: i64): bool;
@external("mududb:component-shim/types", "value-as-boolean")
declare function rawValueAsBoolean(tag: i32, payload1: i64, payload2: i64, result: usize): void;
@external("mududb:component-shim/types", "value-as-int64")
declare function rawValueAsInt64(tag: i32, payload1: i64, payload2: i64, result: usize): void;
@external("mududb:component-shim/types", "value-as-float64")
declare function rawValueAsFloat64(tag: i32, payload1: i64, payload2: i64, result: usize): void;
@external("mududb:component-shim/types", "value-as-text")
declare function rawValueAsText(tag: i32, payload1: i64, payload2: i64, result: usize): void;
@external("mududb:component-shim/types", "value-as-binary")
declare function rawValueAsBinary(tag: i32, payload1: i64, payload2: i64, result: usize): void;
@external("mududb:component-shim/types", "value-as-oid")
declare function rawValueAsOid(tag: i32, payload1: i64, payload2: i64, result: usize): void;

@external("mududb:component-shim/system", "[constructor]value-list")
export declare function witValueListNew(): ResourceHandle;
@external("mududb:component-shim/system", "[method]value-list.bind-named-value")
declare function rawValueListBindNamedValue(self: ResourceHandle, namePtr: usize, nameLen: usize, tag: i32, payload1: i64, payload2: i64): void;
@external("mududb:component-shim/system", "[method]value-list.bind-value")
declare function rawValueListBindValue(self: ResourceHandle, index: i32, tag: i32, payload1: i64, payload2: i64): void;
@external("mududb:component-shim/system", "[method]value-list.len")
export declare function witValueListLen(self: ResourceHandle): u32;
@external("mududb:component-shim/system", "[method]value-list.value")
declare function rawValueListValue(self: ResourceHandle, index: u32, result: usize): void;
@external("mududb:component-shim/system", "[constructor]sql-stmt")
declare function rawSqlStmtNew(ptr: usize, len: usize): ResourceHandle;
@external("mududb:component-shim/system", "[method]result-set.next")
declare function rawResultSetNext(self: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "[method]result-set.current-row")
declare function rawResultSetCurrentRow(self: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "[method]result-set.column-count")
declare function rawResultSetColumnCount(self: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "[method]result-set.column-name")
declare function rawResultSetColumnName(self: ResourceHandle, column: u32, result: usize): void;
@external("mududb:component-shim/system", "[method]result-set.find-column")
declare function rawResultSetFindColumn(self: ResourceHandle, namePtr: usize, nameLen: usize, result: usize): void;
@external("mududb:component-shim/system", "[method]result-set.eof")
declare function rawResultSetEof(self: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "[method]row.is-null")
declare function rawRowIsNull(self: ResourceHandle, column: u32, result: usize): void;
@external("mududb:component-shim/system", "[method]row.is-null-by-name")
declare function rawRowIsNullByName(self: ResourceHandle, namePtr: usize, nameLen: usize, result: usize): void;
@external("mududb:component-shim/system", "[method]row.value")
declare function rawRowValue(self: ResourceHandle, column: u32, result: usize): void;
@external("mududb:component-shim/system", "[method]row.value-by-name")
declare function rawRowValueByName(self: ResourceHandle, namePtr: usize, nameLen: usize, result: usize): void;
@external("mududb:component-shim/system", "open")
declare function rawOpen(uriPtr: usize, uriLen: usize, result: usize): void;
@external("mududb:component-shim/system", "close")
declare function rawClose(hi: u64, lo: u64, result: usize): void;
@external("mududb:component-shim/system", "query")
declare function rawQuery(idHi: u64, idLo: u64, stmt: ResourceHandle, values: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "command")
declare function rawCommand(idHi: u64, idLo: u64, stmt: ResourceHandle, values: ResourceHandle, result: usize): void;
@external("mududb:component-shim/system", "batch")
declare function rawBatch(idHi: u64, idLo: u64, stmt: ResourceHandle, values: ResourceHandle, result: usize): void;

function alloc(size: usize): usize {
  return __new(size, idof<ArrayBuffer>());
}

function utf8Bytes(input: string): ArrayBuffer {
  return String.UTF8.encode(input, false);
}

function bytesPtr(input: Uint8Array): usize {
  return changetype<usize>(input.buffer) + input.byteOffset;
}

function lowerString(input: string, out: usize): void {
  const bytes = utf8Bytes(input);
  const ptr = changetype<usize>(bytes);
  store<u32>(out, <u32>ptr);
  store<u32>(out + 4, <u32>bytes.byteLength);
}

function liftString(ptr: usize, len: usize): string {
  return String.UTF8.decodeUnsafe(ptr, len, true);
}

function liftBytes(ptr: usize, len: usize): Uint8Array {
  const out = new Uint8Array(<i32>len);
  memory.copy(bytesPtr(out), ptr, len);
  return out;
}

export function lowerValue(value: Value, out: usize): void {
  store<u32>(out, <u32>value.kind);
  switch (value.kind) {
    case ValueKind.Boolean:
      store<u32>(out + VALUE_PAYLOAD_OFFSET, value.boolValue ? 1 : 0);
      break;
    case ValueKind.Int64:
      store<i64>(out + VALUE_PAYLOAD_OFFSET, value.int64Value);
      break;
    case ValueKind.Float64:
      store<f64>(out + VALUE_PAYLOAD_OFFSET, value.float64Value);
      break;
    case ValueKind.Text:
      lowerString(value.textValue, out + VALUE_PAYLOAD_OFFSET);
      break;
    case ValueKind.Binary:
      store<u32>(out + VALUE_PAYLOAD_OFFSET, <u32>bytesPtr(value.binaryValue));
      store<u32>(out + VALUE_PAYLOAD_OFFSET + 4, value.binaryValue.length);
      break;
    case ValueKind.ObjectId:
      store<u64>(out + VALUE_PAYLOAD_OFFSET, value.oidValue.hi);
      store<u64>(out + VALUE_PAYLOAD_OFFSET + 8, value.oidValue.lo);
      break;
    default:
      break;
  }
}

function valuePayload1(value: Value): i64 {
  switch (value.kind) {
    case ValueKind.Boolean:
      return value.boolValue ? 1 : 0;
    case ValueKind.Int64:
      return value.int64Value;
    case ValueKind.Float64:
      return reinterpret<i64>(value.float64Value);
    case ValueKind.Text: {
      const bytes = utf8Bytes(value.textValue);
      return (<i64>changetype<usize>(bytes)) | (<i64>bytes.byteLength << 32);
    }
    case ValueKind.Binary:
      return (<i64>bytesPtr(value.binaryValue)) | (<i64>value.binaryValue.length << 32);
    case ValueKind.ObjectId:
      return <i64>value.oidValue.hi;
    default:
      return 0;
  }
}

function valuePayload2(value: Value): i64 {
  if (value.kind == ValueKind.ObjectId) return <i64>value.oidValue.lo;
  return 0;
}

export function liftValue(ptr: usize): Value {
  const tag = load<u32>(ptr);
  const value = new Value(<ValueKind>tag);
  switch (<ValueKind>tag) {
    case ValueKind.Boolean:
      value.boolValue = load<u32>(ptr + VALUE_PAYLOAD_OFFSET) != 0;
      break;
    case ValueKind.Int64:
      value.int64Value = load<i64>(ptr + VALUE_PAYLOAD_OFFSET);
      break;
    case ValueKind.Float64:
      value.float64Value = load<f64>(ptr + VALUE_PAYLOAD_OFFSET);
      break;
    case ValueKind.Text:
      value.textValue = liftString(load<u32>(ptr + VALUE_PAYLOAD_OFFSET), load<u32>(ptr + VALUE_PAYLOAD_OFFSET + 4));
      break;
    case ValueKind.Binary:
      value.binaryValue = liftBytes(load<u32>(ptr + VALUE_PAYLOAD_OFFSET), load<u32>(ptr + VALUE_PAYLOAD_OFFSET + 4));
      break;
    case ValueKind.ObjectId:
      value.oidValue = new Oid(load<u64>(ptr + VALUE_PAYLOAD_OFFSET), load<u64>(ptr + VALUE_PAYLOAD_OFFSET + 8));
      break;
    default:
      break;
  }
  return value;
}

function liftError(ptr: usize): MuduError {
  const messagePtr = load<u32>(ptr + 4);
  const messageLen = load<u32>(ptr + 8);
  const sourcePtr = load<u32>(ptr + 12);
  const sourceLen = load<u32>(ptr + 16);
  const locationPtr = load<u32>(ptr + 20);
  const locationLen = load<u32>(ptr + 24);
  return new MuduError(
    load<u32>(ptr),
    liftString(messagePtr, messageLen),
    liftString(sourcePtr, sourceLen),
    liftString(locationPtr, locationLen),
  );
}

export function lowerError(error: MuduError, out: usize): void {
  store<u32>(out, error.code);
  lowerString(error.message, out + 4);
  lowerString(error.source, out + 12);
  lowerString(error.location, out + 20);
}

function resultIsOk(ptr: usize): bool {
  return load<u32>(ptr) == 0;
}

function resultError(ptr: usize, payloadOffset: usize): MuduError {
  return liftError(ptr + payloadOffset);
}

export function lowerValueListResult(ok: bool, raw: ResourceHandle, error: MuduError): usize {
  const out = alloc(RESULT_ERROR_SIZE);
  store<u32>(out, ok ? 0 : 1);
  if (ok) {
    store<u32>(out + 4, raw);
  } else {
    lowerError(error, out + 4);
  }
  return out;
}

export function witValueNull(): Value {
  const out = alloc(VALUE_SIZE);
  rawValueNull(out);
  return liftValue(out);
}

export function witValueFromBoolean(input: bool): Value {
  const out = alloc(VALUE_SIZE);
  rawValueFromBoolean(input, out);
  return liftValue(out);
}

export function witValueFromInt64(input: i64): Value {
  const out = alloc(VALUE_SIZE);
  rawValueFromInt64(input, out);
  return liftValue(out);
}

export function witValueFromFloat64(input: f64): Value {
  const out = alloc(VALUE_SIZE);
  rawValueFromFloat64(input, out);
  return liftValue(out);
}

export function witValueFromText(input: string): Value {
  const bytes = utf8Bytes(input);
  const out = alloc(VALUE_SIZE);
  rawValueFromText(changetype<usize>(bytes), bytes.byteLength, out);
  return liftValue(out);
}

export function witValueFromBinary(input: Uint8Array): Value {
  const out = alloc(VALUE_SIZE);
  rawValueFromBinary(bytesPtr(input), input.length, out);
  return liftValue(out);
}

export function witValueFromOid(input: Oid): Value {
  const out = alloc(VALUE_SIZE);
  rawValueFromOid(input.hi, input.lo, out);
  return liftValue(out);
}

export function witValueIsNull(input: Value): bool {
  return rawValueIsNull(<i32>input.kind, valuePayload1(input), valuePayload2(input));
}

export function witValueAsBoolean(input: Value): BoolResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsBoolean(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new BoolResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4) != 0)
    : new BoolResult(false, false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witValueAsInt64(input: Value): I64Result {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsInt64(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new I64Result(true, load<i64>(out + ERROR_RESULT_VALUE_OFFSET_8))
    : new I64Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function witValueAsFloat64(input: Value): F64Result {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsFloat64(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new F64Result(true, load<f64>(out + ERROR_RESULT_VALUE_OFFSET_8))
    : new F64Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function witValueAsText(input: Value): StringResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsText(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new StringResult(true, liftString(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : new StringResult(false, "", resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witValueAsBinary(input: Value): BytesResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsBinary(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new BytesResult(true, liftBytes(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : new BytesResult(false, new Uint8Array(0), resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witValueAsOid(input: Value): OidResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawValueAsOid(<i32>input.kind, valuePayload1(input), valuePayload2(input), out);
  return resultIsOk(out)
    ? new OidResult(true, new Oid(load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8), load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8 + 8)))
    : new OidResult(false, new Oid(), resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function witValueListBindNamedValue(self: ResourceHandle, name: string, value: Value): void {
  const bytes = utf8Bytes(name);
  rawValueListBindNamedValue(self, changetype<usize>(bytes), bytes.byteLength, <i32>value.kind, valuePayload1(value), valuePayload2(value));
}

export function witValueListBindValue(self: ResourceHandle, index: i32, value: Value): void {
  rawValueListBindValue(self, index, <i32>value.kind, valuePayload1(value), valuePayload2(value));
}

export function witValueListValue(self: ResourceHandle, index: u32): ValueResult {
  const out = alloc(RESULT_VALUE_SIZE);
  rawValueListValue(self, index, out);
  return resultIsOk(out)
    ? new ValueResult(true, liftValue(out + VALUE_RESULT_VALUE_OFFSET))
    : new ValueResult(false, new Value(), resultError(out, VALUE_RESULT_VALUE_OFFSET));
}

export function witSqlStmtNew(sql: string): ResourceHandle {
  const bytes = utf8Bytes(sql);
  return rawSqlStmtNew(changetype<usize>(bytes), bytes.byteLength);
}

export function witResultSetNext(self: ResourceHandle): BoolResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetNext(self, out);
  return resultIsOk(out)
    ? new BoolResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4) != 0)
    : new BoolResult(false, false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witResultSetCurrentRow(self: ResourceHandle): ResourceResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetCurrentRow(self, out);
  return resultIsOk(out)
    ? new ResourceResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4))
    : new ResourceResult(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witResultSetColumnCount(self: ResourceHandle): U32Result {
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetColumnCount(self, out);
  return resultIsOk(out)
    ? new U32Result(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4))
    : new U32Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witResultSetColumnName(self: ResourceHandle, column: u32): StringResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetColumnName(self, column, out);
  return resultIsOk(out)
    ? new StringResult(true, liftString(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : new StringResult(false, "", resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witResultSetFindColumn(self: ResourceHandle, name: string): U32Result {
  const bytes = utf8Bytes(name);
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetFindColumn(self, changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? new U32Result(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4))
    : new U32Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witResultSetEof(self: ResourceHandle): BoolResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawResultSetEof(self, out);
  return resultIsOk(out)
    ? new BoolResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4) != 0)
    : new BoolResult(false, false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witRowIsNull(self: ResourceHandle, column: u32): BoolResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawRowIsNull(self, column, out);
  return resultIsOk(out)
    ? new BoolResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4) != 0)
    : new BoolResult(false, false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witRowIsNullByName(self: ResourceHandle, name: string): BoolResult {
  const bytes = utf8Bytes(name);
  const out = alloc(RESULT_ERROR_SIZE);
  rawRowIsNullByName(self, changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? new BoolResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4) != 0)
    : new BoolResult(false, false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witRowValue(self: ResourceHandle, column: u32): ValueResult {
  const out = alloc(RESULT_VALUE_SIZE);
  rawRowValue(self, column, out);
  return resultIsOk(out)
    ? new ValueResult(true, liftValue(out + VALUE_RESULT_VALUE_OFFSET))
    : new ValueResult(false, new Value(), resultError(out, VALUE_RESULT_VALUE_OFFSET));
}

export function witRowValueByName(self: ResourceHandle, name: string): ValueResult {
  const bytes = utf8Bytes(name);
  const out = alloc(RESULT_VALUE_SIZE);
  rawRowValueByName(self, changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? new ValueResult(true, liftValue(out + VALUE_RESULT_VALUE_OFFSET))
    : new ValueResult(false, new Value(), resultError(out, VALUE_RESULT_VALUE_OFFSET));
}

export function witOpen(uri: string): OidResult {
  const bytes = utf8Bytes(uri);
  const out = alloc(RESULT_ERROR_SIZE);
  rawOpen(changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? new OidResult(true, new Oid(load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8), load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8 + 8)))
    : new OidResult(false, new Oid(), resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function witClose(id: Oid): VoidResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawClose(id.hi, id.lo, out);
  return resultIsOk(out)
    ? new VoidResult(true)
    : new VoidResult(false, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witQuery(id: Oid, stmt: ResourceHandle, values: ResourceHandle): ResourceResult {
  const out = alloc(RESULT_ERROR_SIZE);
  rawQuery(id.hi, id.lo, stmt, values, out);
  return resultIsOk(out)
    ? new ResourceResult(true, load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4))
    : new ResourceResult(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_4));
}

export function witCommand(id: Oid, stmt: ResourceHandle, values: ResourceHandle): U64Result {
  const out = alloc(RESULT_ERROR_SIZE);
  rawCommand(id.hi, id.lo, stmt, values, out);
  return resultIsOk(out)
    ? new U64Result(true, load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8))
    : new U64Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function witBatch(id: Oid, stmt: ResourceHandle, values: ResourceHandle): U64Result {
  const out = alloc(RESULT_ERROR_SIZE);
  rawBatch(id.hi, id.lo, stmt, values, out);
  return resultIsOk(out)
    ? new U64Result(true, load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8))
    : new U64Result(false, 0, resultError(out, ERROR_RESULT_VALUE_OFFSET_8));
}

export function cabi_realloc(oldPtr: usize, oldSize: usize, align: usize, newSize: usize): usize {
  const newPtr = alloc(newSize);
  if (oldPtr != 0 && oldSize != 0) {
    memory.copy(newPtr, oldPtr, oldSize < newSize ? oldSize : newSize);
  }
  return newPtr;
}

export function _initialize(): void {}
