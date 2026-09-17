// Core DX types for the AssemblyScript guest binding: pure-AS data classes
// plus the MessagePack/uni conversions that used to live behind a composed
// resource ABI. The syscall byte-pipe imports live in `syscall.ts`; the mp2
// procedure byte-pipe helpers live in `procedure.ts`.

import {
  UniDataValue,
  UniDataValueBinary,
  UniDataValueKind,
  UniDataValueScalar,
} from "./generated/UniDataValue";
import {
  UniScalarValue,
  UniScalarValueBlob,
  UniScalarValueBool,
  UniScalarValueChar,
  UniScalarValueDate,
  UniScalarValueF32,
  UniScalarValueF64,
  UniScalarValueI16,
  UniScalarValueI32,
  UniScalarValueI64,
  UniScalarValueI8,
  UniScalarValueKind,
  UniScalarValueNull,
  UniScalarValueNumeric,
  UniScalarValueString,
  UniScalarValueTime,
  UniScalarValueTimestamp,
  UniScalarValueTimestampTz,
  UniScalarValueU16,
  UniScalarValueU32,
  UniScalarValueU64,
  UniScalarValueU8,
  UniScalarValueU128,
} from "./generated/UniScalarValue";

// Mudu error codes (`mudu::error::ErrorCode`), mirroring the C# guest's
// `ErrorCodes` for procedure error results.
export const ERROR_INTERNAL: u32 = 50000;
export const ERROR_ENTITY_NOT_FOUND: u32 = 50009;
export const ERROR_DOMAIN_VIOLATION: u32 = 50017;
export const ERROR_INVALID_ARGUMENT: u32 = 50029;

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
    return new Value(ValueKind.Null);
  }

  static boolean(input: bool): Value {
    const value = new Value(ValueKind.Boolean);
    value.boolValue = input;
    return value;
  }

  static int64(input: i64): Value {
    const value = new Value(ValueKind.Int64);
    value.int64Value = input;
    return value;
  }

  static float64(input: f64): Value {
    const value = new Value(ValueKind.Float64);
    value.float64Value = input;
    return value;
  }

  static text(input: string): Value {
    const value = new Value(ValueKind.Text);
    value.textValue = input;
    return value;
  }

  static binary(input: Uint8Array): Value {
    const value = new Value(ValueKind.Binary);
    value.binaryValue = input;
    return value;
  }

  static objectId(input: Oid): Value {
    const value = new Value(ValueKind.ObjectId);
    value.oidValue = input;
    return value;
  }

  isNull(): bool {
    return this.kind == ValueKind.Null;
  }

  asBoolean(): bool {
    if (this.kind != ValueKind.Boolean) throw typeError("boolean");
    return this.boolValue;
  }

  asInt64(): i64 {
    if (this.kind != ValueKind.Int64) throw typeError("int64");
    return this.int64Value;
  }

  asFloat64(): f64 {
    if (this.kind != ValueKind.Float64) throw typeError("float64");
    return this.float64Value;
  }

  asText(): string {
    if (this.kind != ValueKind.Text) throw typeError("text");
    return this.textValue;
  }

  asBinary(): Uint8Array {
    if (this.kind != ValueKind.Binary) throw typeError("binary");
    return this.binaryValue;
  }

  asObjectId(): Oid {
    if (this.kind != ValueKind.ObjectId) throw typeError("oid");
    return this.oidValue;
  }

  // Encode as a `UniDataValue` for the wire. Mirrors the mappings the host's
  // `DataValue` conversions imply: boolean travels as i32 (0/1), object-id as
  // a big-endian u128 byte string, binary as the scalar blob case (which the
  // generated codec emits as the pinned int-array form).
  toUniDataValue(): UniDataValue {
    switch (this.kind) {
      case ValueKind.Null:
        return scalarOf(new UniScalarValueNull());
      case ValueKind.Boolean: {
        const scalar = new UniScalarValueI32();
        scalar.inner = this.boolValue ? 1 : 0;
        return scalarOf(scalar);
      }
      case ValueKind.Int64: {
        const scalar = new UniScalarValueI64();
        scalar.inner = this.int64Value;
        return scalarOf(scalar);
      }
      case ValueKind.Float64: {
        const scalar = new UniScalarValueF64();
        scalar.inner = this.float64Value;
        return scalarOf(scalar);
      }
      case ValueKind.Text: {
        const scalar = new UniScalarValueString();
        scalar.inner = this.textValue;
        return scalarOf(scalar);
      }
      case ValueKind.Binary: {
        const scalar = new UniScalarValueBlob();
        scalar.inner = this.binaryValue;
        return scalarOf(scalar);
      }
      case ValueKind.ObjectId: {
        const scalar = new UniScalarValueU128();
        scalar.inner = oidToBeBytes(this.oidValue);
        return scalarOf(scalar);
      }
      default:
        throw new Error("unsupported value kind");
    }
  }

  // Decode a `UniDataValue` coming from the host. Mirrors the host-side
  // `DataValue` conversions: all signed/unsigned ints up to 64 bits become
  // int64, f32/f64 become float64, the string-like scalars become text, blob
  // (and the binary data-value case) become binary, u128 becomes object-id.
  static fromUniDataValue(input: UniDataValue): Value {
    if (input.kind == UniDataValueKind.Scalar) {
      const scalar = (input as UniDataValueScalar).inner;
      switch (scalar.kind) {
        case UniScalarValueKind.Null:
          return Value.null();
        case UniScalarValueKind.Bool:
          return Value.boolean((scalar as UniScalarValueBool).inner);
        case UniScalarValueKind.U8:
          return Value.int64((scalar as UniScalarValueU8).inner as i64);
        case UniScalarValueKind.I8:
          return Value.int64((scalar as UniScalarValueI8).inner as i64);
        case UniScalarValueKind.U16:
          return Value.int64((scalar as UniScalarValueU16).inner as i64);
        case UniScalarValueKind.I16:
          return Value.int64((scalar as UniScalarValueI16).inner as i64);
        case UniScalarValueKind.U32:
          return Value.int64((scalar as UniScalarValueU32).inner as i64);
        case UniScalarValueKind.I32:
          return Value.int64((scalar as UniScalarValueI32).inner as i64);
        case UniScalarValueKind.U64:
          return Value.int64((scalar as UniScalarValueU64).inner as i64);
        case UniScalarValueKind.I64:
          return Value.int64((scalar as UniScalarValueI64).inner);
        case UniScalarValueKind.F32:
          return Value.float64((scalar as UniScalarValueF32).inner as f64);
        case UniScalarValueKind.F64:
          return Value.float64((scalar as UniScalarValueF64).inner);
        case UniScalarValueKind.Char:
          return Value.text((scalar as UniScalarValueChar).inner);
        case UniScalarValueKind.String:
          return Value.text((scalar as UniScalarValueString).inner);
        case UniScalarValueKind.Numeric:
          return Value.text((scalar as UniScalarValueNumeric).inner);
        case UniScalarValueKind.Date:
          return Value.text((scalar as UniScalarValueDate).inner);
        case UniScalarValueKind.Time:
          return Value.text((scalar as UniScalarValueTime).inner);
        case UniScalarValueKind.Timestamp:
          return Value.text((scalar as UniScalarValueTimestamp).inner);
        case UniScalarValueKind.TimestampTz:
          return Value.text((scalar as UniScalarValueTimestampTz).inner);
        case UniScalarValueKind.Blob:
          return Value.binary((scalar as UniScalarValueBlob).inner);
        case UniScalarValueKind.U128:
          return Value.objectId(oidFromBeBytes((scalar as UniScalarValueU128).inner));
        default:
          throw new Error("unsupported scalar value kind on the wire");
      }
    }
    if (input.kind == UniDataValueKind.Binary) {
      return Value.binary((input as UniDataValueBinary).inner);
    }
    if (input.kind == UniDataValueKind.Array || input.kind == UniDataValueKind.Record) {
      throw new Error("array and record data values are not supported");
    }
    throw new Error("unsupported data value kind on the wire");
  }
}

function scalarOf(scalar: UniScalarValue): UniDataValue {
  const value = new UniDataValueScalar();
  value.inner = scalar;
  return value;
}

function typeError(expected: string): Error {
  return new Error("value is not " + expected);
}

// u128 wire form is 16 big-endian bytes (`to_be_bytes` on the host), so the
// oid's high half occupies the first 8 bytes.
export function oidToBeBytes(oid: Oid): Uint8Array {
  const out = new Uint8Array(16);
  for (let i = 0; i < 8; i++) {
    out[i] = (oid.hi >> (<u64>(7 - i) * 8)) as u8;
    out[8 + i] = (oid.lo >> (<u64>(7 - i) * 8)) as u8;
  }
  return out;
}

export function oidFromBeBytes(bytes: Uint8Array): Oid {
  if (bytes.length != 16) {
    throw new Error("u128 wire value must be 16 bytes");
  }
  let hi: u64 = 0;
  let lo: u64 = 0;
  for (let i = 0; i < 8; i++) {
    hi = (hi << 8) | (bytes[i] as u64);
    lo = (lo << 8) | (bytes[8 + i] as u64);
  }
  return new Oid(hi, lo);
}

export function alloc(size: usize): usize {
  return __new(size, idof<ArrayBuffer>());
}

export function utf8Bytes(input: string): ArrayBuffer {
  return String.UTF8.encode(input, false);
}

export function bytesPtr(input: Uint8Array): usize {
  return changetype<usize>(input.buffer) + input.byteOffset;
}

export function liftString(ptr: usize, len: usize): string {
  return String.UTF8.decodeUnsafe(ptr, len, true);
}

export function liftBytes(ptr: usize, len: usize): Uint8Array {
  const out = new Uint8Array(<i32>len);
  if (len > 0) {
    memory.copy(bytesPtr(out), ptr, len);
  }
  return out;
}

export function cabi_realloc(oldPtr: usize, oldSize: usize, align: usize, newSize: usize): usize {
  const newPtr = alloc(newSize);
  if (oldPtr != 0 && oldSize != 0) {
    memory.copy(newPtr, oldPtr, oldSize < newSize ? oldSize : newSize);
  }
  return newPtr;
}

export function _initialize(): void {}
