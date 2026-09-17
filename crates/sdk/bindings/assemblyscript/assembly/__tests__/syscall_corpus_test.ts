// Corpus tests for the mgen-generated AssemblyScript MSSP syscall codec
// (`../generated/`). This file is a test entry only; it is not part of the
// public `index.ts` exports. It is compiled and driven by two Node runners:
//   - run_syscall_corpus_test.mjs against
//     crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin
//     (47 frames: one request + one ok response per message kind 1..23, plus
//     a trailing `get` UniError response),
//   - run_lenient_decode_test.mjs against
//     crates/db-kernel/testing/fixtures/golden/v1/lenient_decode_v1.bin
//     (8 hand-built NON-canonical frames).
// Semantic expectations are NOT hardcoded here: the `decode*Json` exports
// decode a frame with the generated AS codec (inside wasm) and serialize the
// decoded values into the `expect` object shape of the sidecars
// (syscall_payload_v1_all.json / lenient_decode_v1.json, conventions:
// u64/i64 as decimal strings, bytes as lowercase hex, UniOid as {"h","l"},
// unit as {"unit": true}, relation cells as hex-or-null). The Node runners
// JSON.parse the result and deep-compare it against the sidecar, which is
// the single source of truth. Only the deterministic encode INPUTS (pinned
// byte-exactly against the fixture frames) are still constructed here.

import {
  MessageKind,
  decodeHeader,
  encodeQueryRequest, decodeQueryRequest, QueryResult, encodeQueryResult, decodeQueryResult,
  encodeCommandRequest, decodeCommandRequest, CommandResult, encodeCommandResult, decodeCommandResult,
  encodeBatchRequest, decodeBatchRequest, BatchResult, encodeBatchResult, decodeBatchResult,
  encodeOpenSessionRequest, decodeOpenSessionRequest, OpenSessionResult, encodeOpenSessionResult, decodeOpenSessionResult,
  encodeCloseSessionRequest, decodeCloseSessionRequest, CloseSessionResult, encodeCloseSessionResult, decodeCloseSessionResult,
  encodeGetRequest, decodeGetRequest, GetResult, encodeGetResult, decodeGetResult,
  encodePutRequest, decodePutRequest, PutResult, encodePutResult, decodePutResult,
  encodeDeleteRequest, decodeDeleteRequest, DeleteResult, encodeDeleteResult, decodeDeleteResult,
  RangeResultItem, encodeRangeRequest, decodeRangeRequest, RangeResult, encodeRangeResult, decodeRangeResult,
  encodeFsOpenRequest, decodeFsOpenRequest, FsOpenResult, encodeFsOpenResult, decodeFsOpenResult,
  encodeFsCloseRequest, decodeFsCloseRequest, FsCloseResult, encodeFsCloseResult, decodeFsCloseResult,
  encodeFsReadRequest, decodeFsReadRequest, FsReadResult, encodeFsReadResult, decodeFsReadResult,
  encodeFsWriteRequest, decodeFsWriteRequest, FsWriteResult, encodeFsWriteResult, decodeFsWriteResult,
  encodeFsPreadRequest, decodeFsPreadRequest, FsPreadResult, encodeFsPreadResult, decodeFsPreadResult,
  encodeFsPwriteRequest, decodeFsPwriteRequest, FsPwriteResult, encodeFsPwriteResult, decodeFsPwriteResult,
  encodeFsLseekRequest, decodeFsLseekRequest, FsLseekResult, encodeFsLseekResult, decodeFsLseekResult,
  encodeFsFstatRequest, decodeFsFstatRequest, FsFstatResult, encodeFsFstatResult, decodeFsFstatResult,
  encodeFsStatRequest, decodeFsStatRequest, FsStatResult, encodeFsStatResult, decodeFsStatResult,
  encodeFsFsyncRequest, decodeFsFsyncRequest, FsFsyncResult, encodeFsFsyncResult, decodeFsFsyncResult,
  encodeFsReaddirRequest, decodeFsReaddirRequest, FsReaddirResult, encodeFsReaddirResult, decodeFsReaddirResult,
  RelationGetKeyItem, encodeRelationGetRequest, decodeRelationGetRequest, RelationGetResult, encodeRelationGetResult, decodeRelationGetResult,
  RelationUpdateKeyItem, RelationUpdateValuesItem, RelationUpdateDeltasItem,
  encodeRelationUpdateRequest, decodeRelationUpdateRequest, RelationUpdateResult, encodeRelationUpdateResult, decodeRelationUpdateResult,
  RelationInsertKeyItem, RelationInsertValuesItem,
  encodeRelationInsertRequest, decodeRelationInsertRequest, RelationInsertResult, encodeRelationInsertResult, decodeRelationInsertResult,
} from "../generated/UniSyscall";
import { UniOid } from "../generated/UniOid";
import { UniQueryArgv } from "../generated/UniQueryArgv";
import { UniSqlStmt } from "../generated/UniSqlStmt";
import { UniSqlParam } from "../generated/UniSqlParam";
import { UniCommandArgv } from "../generated/UniCommandArgv";
import { UniCommandResult } from "../generated/UniCommandResult";
import { UniQueryResult } from "../generated/UniQueryResult";
import { UniFsOpenArgv } from "../generated/UniFsOpenArgv";
import { UniFsStat } from "../generated/UniFsStat";
import { UniFsDirent, UniFsDirentCodec } from "../generated/UniFsDirent";
import { UniError, UniErrorCodec } from "../generated/UniError";
import { UniResultSet, UniResultSetCodec } from "../generated/UniResultSet";
import { UniRecordField } from "../generated/UniRecordType";
import { UniTupleRow } from "../generated/UniTupleRow";
import { UniDataValue, UniDataValueScalar } from "../generated/UniDataValue";
import { UniScalarValueU32 } from "../generated/UniScalarValue";
import { UniDataType, UniDataTypeKind, UniDataTypeScalar } from "../generated/UniDataType";
import { UniScalar } from "../generated/UniScalar";
import { MpackWriter, MpackReader } from "../mpack";

// ---- helpers ----

function oid(l: u64): UniOid {
  const o = new UniOid();
  o.h = 0;
  o.l = l;
  return o;
}

// ASCII-only helper: the corpus strings are pure ASCII, so one byte per char.
function ascii(s: string): Uint8Array {
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = <u8>s.charCodeAt(i);
  return out;
}

function bytesEq(a: Uint8Array, b: Uint8Array): bool {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function queryArgv(l: u64, sql: string): UniQueryArgv {
  const argv = new UniQueryArgv();
  argv.oid = oid(l);
  argv.query = new UniSqlStmt();
  argv.query.sql_string = sql;
  argv.param_list = new UniSqlParam();
  return argv;
}

function commandArgv(l: u64, sql: string): UniCommandArgv {
  const argv = new UniCommandArgv();
  argv.oid = oid(l);
  argv.command = new UniSqlStmt();
  argv.command.sql_string = sql;
  argv.param_list = new UniSqlParam();
  return argv;
}

function fsOpenArgv(): UniFsOpenArgv {
  const argv = new UniFsOpenArgv();
  argv.session = oid(10);
  argv.oid = oid(11);
  argv.path = "docs/a.txt";
  argv.flags = 2;
  return argv;
}

function goldenFsStat(): UniFsStat {
  const stat = new UniFsStat();
  stat.oid = oid(5);
  stat.generation = 1;
  stat.entry = "";
  stat.length = 100;
  stat.state = 1;
  return stat;
}

function goldenDirents(): Array<UniFsDirent> {
  const a = new UniFsDirent();
  a.name = "a.txt";
  a.is_dir = false;
  a.length = 3;
  const b = new UniFsDirent();
  b.name = "docs";
  b.is_dir = true;
  b.length = 0;
  return [a, b];
}

function relationGetKey(): Array<RelationGetKeyItem> {
  const a = new RelationGetKeyItem();
  a.f0 = 1;
  a.f1 = Uint8Array.wrap(String.UTF8.encode("\x01"));
  const b = new RelationGetKeyItem();
  b.f0 = 2;
  b.f1 = Uint8Array.wrap(String.UTF8.encode("\x02\x03"));
  return [a, b];
}

function relationUpdateKey(): Array<RelationUpdateKeyItem> {
  const a = new RelationUpdateKeyItem();
  a.f0 = 1;
  a.f1 = Uint8Array.wrap(String.UTF8.encode("\x01"));
  return [a];
}

function relationUpdateValues(): Array<RelationUpdateValuesItem> {
  const a = new RelationUpdateValuesItem();
  a.f0 = 2;
  a.f1 = Uint8Array.wrap(String.UTF8.encode("\x0A"));
  return [a];
}

function relationUpdateDeltas(): Array<RelationUpdateDeltasItem> {
  const a = new RelationUpdateDeltasItem();
  a.f0 = 3;
  a.f1 = 0;
  a.f2 = Uint8Array.wrap(String.UTF8.encode("\x05"));
  return [a];
}

function relationInsertKey(): Array<RelationInsertKeyItem> {
  const a = new RelationInsertKeyItem();
  a.f0 = 1;
  a.f1 = Uint8Array.wrap(String.UTF8.encode("\x01"));
  return [a];
}

function relationInsertValues(): Array<RelationInsertValuesItem> {
  const a = new RelationInsertValuesItem();
  a.f0 = 2;
  a.f1 = Uint8Array.wrap(String.UTF8.encode("\x0A"));
  return [a];
}

// The trailing corpus frame: get result Err(NotFound, "no such entry"). On
// the wire the UniError is {1: 2, 2: "no such entry", 3: "\"None\"", 4: "",
// 5: []} (err_src is the host's JSON form of ErrorSource::None; err_details
// encodes as a MessagePack ARRAY, not bin — both quirks pinned on purpose).
function goldenErr(): UniError {
  const err = new UniError();
  err.err_code = 2;
  err.err_msg = "no such entry";
  err.err_src = "\"None\"";
  err.err_loc = "";
  err.err_details = new Uint8Array(0);
  return err;
}

// ---- sidecar `expect` JSON serialization (see the file header; the Node
// runners deep-compare the parsed result against the sidecar) ----

function jsonStr(s: string): string {
  let out = "\"";
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c === 0x22) {
      out += "\\\"";
    } else if (c === 0x5c) {
      out += "\\\\";
    } else if (c < 0x20) {
      let h = c.toString(16);
      while (h.length < 4) h = "0" + h;
      out += "\\u" + h;
    } else {
      out += s.charAt(i);
    }
  }
  return out + "\"";
}

function boolJson(v: bool): string {
  return v ? "true" : "false";
}

function hexOf(b: Uint8Array): string {
  let out = "";
  for (let i = 0; i < b.length; i++) {
    const h = b[i].toString(16);
    out += h.length === 1 ? "0" + h : h;
  }
  return out;
}

function joinJson(items: Array<string>): string {
  let out = "";
  for (let i = 0; i < items.length; i++) {
    if (i > 0) out += ",";
    out += items[i];
  }
  return out;
}

function oidJson(o: UniOid): string {
  return "{\"h\":\"" + o.h.toString() + "\",\"l\":\"" + o.l.toString() + "\"}";
}

function attrDatumJson(attr: u64, datum: Uint8Array): string {
  return "{\"attr\":\"" + attr.toString() + "\",\"datum_hex\":\"" + hexOf(datum) + "\"}";
}

function sqlParamsJson(params: UniSqlParam): string {
  // The sidecar conventions do not define a rendering for UniDataValue
  // params; every corpus/lenient frame carries an empty parameter list.
  if (params.params.length !== 0) {
    throw new Error("sql params serialization not covered by the sidecar conventions");
  }
  return "[]";
}

function sqlArgvJson(o: UniOid, sql: string, params: UniSqlParam): string {
  return "{\"oid\":" + oidJson(o)
    + ",\"sql\":" + jsonStr(sql)
    + ",\"params\":" + sqlParamsJson(params) + "}";
}

function scalarName(s: UniScalar): string {
  switch (s) {
    case UniScalar.Bool: return "bool";
    case UniScalar.U8: return "u8";
    case UniScalar.I8: return "i8";
    case UniScalar.U16: return "u16";
    case UniScalar.I16: return "i16";
    case UniScalar.U32: return "u32";
    case UniScalar.I32: return "i32";
    case UniScalar.U64: return "u64";
    case UniScalar.U128: return "u128";
    case UniScalar.I64: return "i64";
    case UniScalar.I128: return "i128";
    case UniScalar.F32: return "f32";
    case UniScalar.F64: return "f64";
    case UniScalar.Char: return "char";
    case UniScalar.String: return "string";
    case UniScalar.Blob: return "blob";
    case UniScalar.Numeric: return "numeric";
    case UniScalar.Date: return "date";
    case UniScalar.Time: return "time";
    case UniScalar.Timestamp: return "timestamp";
    case UniScalar.TimestampTz: return "timestamp_tz";
    default: throw new Error(`unknown scalar ${s}`);
  }
}

function dataTypeName(t: UniDataType): string {
  if (t.kind === UniDataTypeKind.Scalar) {
    // A UniDataType left at its proto3-style default (an absent variant field)
    // is the base class, not a UniDataTypeScalar instance; its scalar value
    // defaults to Bool, matching the host's scalar(bool) rendering.
    const s = t instanceof UniDataTypeScalar ? (t as UniDataTypeScalar).inner : UniScalar.Bool;
    return "scalar(" + scalarName(s) + ")";
  }
  throw new Error("field_type serialization not covered by the sidecar conventions");
}

function recordFieldJson(f: UniRecordField): string {
  const attrs = new Array<string>(f.field_attrs.length);
  for (let i = 0; i < f.field_attrs.length; i++) {
    const a = f.field_attrs[i];
    attrs[i] = "{\"attr_name\":" + jsonStr(a.attr_name) + ",\"attr_value\":" + jsonStr(a.attr_value) + "}";
  }
  return "{\"field_name\":" + jsonStr(f.field_name)
    + ",\"field_type\":\"" + dataTypeName(f.field_type) + "\""
    + ",\"field_attrs\":[" + joinJson(attrs) + "]}";
}

function queryResultJson(r: UniQueryResult): string {
  const fields = new Array<string>(r.tuple_desc.record_fields.length);
  for (let i = 0; i < r.tuple_desc.record_fields.length; i++) {
    fields[i] = recordFieldJson(r.tuple_desc.record_fields[i]);
  }
  // The sidecar conventions do not define a rendering for query rows; every
  // corpus/lenient frame carries an empty row set.
  if (r.result_set.row_set.length !== 0) {
    throw new Error("query row serialization not covered by the sidecar conventions");
  }
  return "{\"record_name\":" + jsonStr(r.tuple_desc.record_name)
    + ",\"fields\":[" + joinJson(fields) + "]"
    + ",\"eof\":" + boolJson(r.result_set.eof)
    + ",\"rows\":[]"
    + ",\"cursor_hex\":\"" + hexOf(r.result_set.cursor) + "\"}";
}

function fsStatJson(s: UniFsStat): string {
  return "{\"oid\":" + oidJson(s.oid)
    + ",\"generation\":\"" + s.generation.toString() + "\""
    + ",\"entry\":" + jsonStr(s.entry)
    + ",\"length\":\"" + s.length.toString() + "\""
    + ",\"state\":" + s.state.toString() + "}";
}

function direntsJson(entries: Array<UniFsDirent>): string {
  const items = new Array<string>(entries.length);
  for (let i = 0; i < entries.length; i++) {
    const d = entries[i];
    items[i] = "{\"name\":" + jsonStr(d.name)
      + ",\"is_dir\":" + boolJson(d.is_dir)
      + ",\"length\":\"" + d.length.toString() + "\"}";
  }
  return "[" + joinJson(items) + "]";
}

function relationRowJson(row: Array<Uint8Array | null> | null): string {
  if (row === null) return "null";
  const cells = new Array<string>(row!.length);
  for (let i = 0; i < row!.length; i++) {
    const cell = row![i];
    cells[i] = cell === null ? "null" : "\"" + hexOf(cell!) + "\"";
  }
  return "[" + joinJson(cells) + "]";
}

function errJson(err: UniError): string {
  return "{\"err_code\":" + err.err_code.toString()
    + ",\"err_msg\":" + jsonStr(err.err_msg)
    + ",\"err_src\":" + jsonStr(err.err_src)
    + ",\"err_loc\":" + jsonStr(err.err_loc)
    + ",\"err_details_hex\":\"" + hexOf(err.err_details) + "\"}";
}

const UNIT_JSON = "{\"unit\":true}";

// ---- header routing ----

export function headerKind(frame: Uint8Array): i32 {
  return decodeHeader(frame) as i32;
}

// ---- request frames: encode from the documented inputs ----

export function encodeRequest(kind: i32): Uint8Array {
  switch (kind) {
    case MessageKind.Query: return encodeQueryRequest(queryArgv(1, "select 1"));
    case MessageKind.Command: return encodeCommandRequest(commandArgv(2, "update t set a = 1"));
    case MessageKind.Batch: return encodeBatchRequest(commandArgv(3, "insert into t values (1)"));
    case MessageKind.OpenSession: return encodeOpenSessionRequest(oid(4));
    case MessageKind.CloseSession: return encodeCloseSessionRequest(oid(5));
    case MessageKind.Get: return encodeGetRequest(oid(6), ascii("k1"));
    case MessageKind.Put: return encodePutRequest(oid(7), ascii("k1"), ascii("v1"));
    case MessageKind.Delete: return encodeDeleteRequest(oid(8), ascii("k1"));
    case MessageKind.Range: return encodeRangeRequest(oid(9), ascii("a"), ascii("z"));
    case MessageKind.FsOpen: return encodeFsOpenRequest(fsOpenArgv());
    case MessageKind.FsClose: return encodeFsCloseRequest(3);
    case MessageKind.FsRead: return encodeFsReadRequest(3, 4);
    case MessageKind.FsWrite: return encodeFsWriteRequest(3, ascii("hi"));
    case MessageKind.FsPread: return encodeFsPreadRequest(3, 8, 4);
    case MessageKind.FsPwrite: return encodeFsPwriteRequest(3, 8, ascii("hi"));
    case MessageKind.FsLseek: return encodeFsLseekRequest(3, -2, 1);
    case MessageKind.FsFstat: return encodeFsFstatRequest(3);
    case MessageKind.FsStat: return encodeFsStatRequest(oid(18), "a");
    case MessageKind.FsFsync: return encodeFsFsyncRequest(3);
    case MessageKind.FsReaddir: return encodeFsReaddirRequest(oid(20), "d");
    case MessageKind.RelationGet:
      return encodeRelationGetRequest(oid(21), "t", relationGetKey(), [3, 4]);
    case MessageKind.RelationUpdate:
      return encodeRelationUpdateRequest(oid(22), "t", relationUpdateKey(), relationUpdateValues(), relationUpdateDeltas());
    case MessageKind.RelationInsert:
      return encodeRelationInsertRequest(oid(23), "t", relationInsertKey(), relationInsertValues());
    default:
      throw new Error(`unknown request kind ${kind}`);
  }
}

// ---- request frames: decode and serialize to the sidecar expect shape ----

export function decodeRequestJson(kind: i32, frame: Uint8Array): string {
  switch (kind) {
    case MessageKind.Query: {
      const v = decodeQueryRequest(frame);
      return sqlArgvJson(v.oid, v.query.sql_string, v.param_list);
    }
    case MessageKind.Command: {
      const v = decodeCommandRequest(frame);
      return sqlArgvJson(v.oid, v.command.sql_string, v.param_list);
    }
    case MessageKind.Batch: {
      const v = decodeBatchRequest(frame);
      return sqlArgvJson(v.oid, v.command.sql_string, v.param_list);
    }
    case MessageKind.OpenSession:
      return "{\"worker_oid\":" + oidJson(decodeOpenSessionRequest(frame)) + "}";
    case MessageKind.CloseSession:
      return "{\"oid\":" + oidJson(decodeCloseSessionRequest(frame)) + "}";
    case MessageKind.Get: {
      const v = decodeGetRequest(frame);
      return "{\"oid\":" + oidJson(v.oid) + ",\"key_hex\":\"" + hexOf(v.key) + "\"}";
    }
    case MessageKind.Put: {
      const v = decodePutRequest(frame);
      return "{\"oid\":" + oidJson(v.oid)
        + ",\"key_hex\":\"" + hexOf(v.key) + "\""
        + ",\"value_hex\":\"" + hexOf(v.value) + "\"}";
    }
    case MessageKind.Delete: {
      const v = decodeDeleteRequest(frame);
      return "{\"oid\":" + oidJson(v.oid) + ",\"key_hex\":\"" + hexOf(v.key) + "\"}";
    }
    case MessageKind.Range: {
      const v = decodeRangeRequest(frame);
      return "{\"oid\":" + oidJson(v.oid)
        + ",\"start_hex\":\"" + hexOf(v.start) + "\""
        + ",\"end_hex\":\"" + hexOf(v.end) + "\"}";
    }
    case MessageKind.FsOpen: {
      const v = decodeFsOpenRequest(frame);
      return "{\"session\":" + oidJson(v.session)
        + ",\"oid\":" + oidJson(v.oid)
        + ",\"path\":" + jsonStr(v.path)
        + ",\"flags\":" + v.flags.toString() + "}";
    }
    case MessageKind.FsClose:
      return "{\"fd\":" + decodeFsCloseRequest(frame).toString() + "}";
    case MessageKind.FsRead: {
      const v = decodeFsReadRequest(frame);
      return "{\"fd\":" + v.fd.toString() + ",\"len\":" + v.len.toString() + "}";
    }
    case MessageKind.FsWrite: {
      const v = decodeFsWriteRequest(frame);
      return "{\"fd\":" + v.fd.toString() + ",\"data_hex\":\"" + hexOf(v.data) + "\"}";
    }
    case MessageKind.FsPread: {
      const v = decodeFsPreadRequest(frame);
      return "{\"fd\":" + v.fd.toString()
        + ",\"offset\":\"" + v.offset.toString() + "\""
        + ",\"len\":" + v.len.toString() + "}";
    }
    case MessageKind.FsPwrite: {
      const v = decodeFsPwriteRequest(frame);
      return "{\"fd\":" + v.fd.toString()
        + ",\"offset\":\"" + v.offset.toString() + "\""
        + ",\"data_hex\":\"" + hexOf(v.data) + "\"}";
    }
    case MessageKind.FsLseek: {
      const v = decodeFsLseekRequest(frame);
      return "{\"fd\":" + v.fd.toString()
        + ",\"offset\":\"" + v.offset.toString() + "\""
        + ",\"whence\":" + v.whence.toString() + "}";
    }
    case MessageKind.FsFstat:
      return "{\"fd\":" + decodeFsFstatRequest(frame).toString() + "}";
    case MessageKind.FsStat: {
      const v = decodeFsStatRequest(frame);
      return "{\"oid\":" + oidJson(v.oid) + ",\"path\":" + jsonStr(v.path) + "}";
    }
    case MessageKind.FsFsync:
      return "{\"fd\":" + decodeFsFsyncRequest(frame).toString() + "}";
    case MessageKind.FsReaddir: {
      const v = decodeFsReaddirRequest(frame);
      return "{\"oid\":" + oidJson(v.oid) + ",\"path\":" + jsonStr(v.path) + "}";
    }
    case MessageKind.RelationGet: {
      const v = decodeRelationGetRequest(frame);
      const key = new Array<string>(v.key.length);
      for (let i = 0; i < v.key.length; i++) key[i] = attrDatumJson(v.key[i].f0, v.key[i].f1);
      const sel = new Array<string>(v.select.length);
      for (let i = 0; i < v.select.length; i++) sel[i] = "\"" + v.select[i].toString() + "\"";
      return "{\"oid\":" + oidJson(v.oid)
        + ",\"table\":" + jsonStr(v.table)
        + ",\"key\":[" + joinJson(key) + "]"
        + ",\"select\":[" + joinJson(sel) + "]}";
    }
    case MessageKind.RelationUpdate: {
      const v = decodeRelationUpdateRequest(frame);
      const key = new Array<string>(v.key.length);
      for (let i = 0; i < v.key.length; i++) key[i] = attrDatumJson(v.key[i].f0, v.key[i].f1);
      const values = new Array<string>(v.values.length);
      for (let i = 0; i < v.values.length; i++) values[i] = attrDatumJson(v.values[i].f0, v.values[i].f1);
      const deltas = new Array<string>(v.deltas.length);
      for (let i = 0; i < v.deltas.length; i++) {
        const d = v.deltas[i];
        deltas[i] = "{\"attr\":\"" + d.f0.toString() + "\""
          + ",\"op\":" + d.f1.toString()
          + ",\"datum_hex\":\"" + hexOf(d.f2) + "\"}";
      }
      return "{\"oid\":" + oidJson(v.oid)
        + ",\"table\":" + jsonStr(v.table)
        + ",\"key\":[" + joinJson(key) + "]"
        + ",\"values\":[" + joinJson(values) + "]"
        + ",\"deltas\":[" + joinJson(deltas) + "]}";
    }
    case MessageKind.RelationInsert: {
      const v = decodeRelationInsertRequest(frame);
      const key = new Array<string>(v.key.length);
      for (let i = 0; i < v.key.length; i++) key[i] = attrDatumJson(v.key[i].f0, v.key[i].f1);
      const values = new Array<string>(v.values.length);
      for (let i = 0; i < v.values.length; i++) values[i] = attrDatumJson(v.values[i].f0, v.values[i].f1);
      return "{\"oid\":" + oidJson(v.oid)
        + ",\"table\":" + jsonStr(v.table)
        + ",\"key\":[" + joinJson(key) + "]"
        + ",\"values\":[" + joinJson(values) + "]}";
    }
    default:
      throw new Error(`unknown request kind ${kind}`);
  }
}

// ---- ok response frames: encode from the documented values ----

export function encodeOkResponse(kind: i32): Uint8Array {
  switch (kind) {
    case MessageKind.Query:
      return encodeQueryResult(QueryResult.ok(new UniQueryResult()));
    case MessageKind.Command: {
      const r = new UniCommandResult();
      r.affected_rows = 3;
      return encodeCommandResult(CommandResult.ok(r));
    }
    case MessageKind.Batch: {
      const r = new UniCommandResult();
      r.affected_rows = 2;
      return encodeBatchResult(BatchResult.ok(r));
    }
    case MessageKind.OpenSession:
      return encodeOpenSessionResult(OpenSessionResult.ok(oid(4)));
    case MessageKind.CloseSession:
      return encodeCloseSessionResult(CloseSessionResult.ok());
    case MessageKind.Get:
      return encodeGetResult(GetResult.ok(ascii("v1")));
    case MessageKind.Put:
      return encodePutResult(PutResult.ok());
    case MessageKind.Delete:
      return encodeDeleteResult(DeleteResult.ok());
    case MessageKind.Range: {
      const a = new RangeResultItem();
      a.f0 = ascii("a");
      a.f1 = ascii("1");
      const b = new RangeResultItem();
      b.f0 = ascii("b");
      b.f1 = ascii("2");
      return encodeRangeResult(RangeResult.ok([a, b]));
    }
    case MessageKind.FsOpen:
      return encodeFsOpenResult(FsOpenResult.ok(9));
    case MessageKind.FsClose:
      return encodeFsCloseResult(FsCloseResult.ok());
    case MessageKind.FsRead:
      return encodeFsReadResult(FsReadResult.ok(ascii("hi")));
    case MessageKind.FsWrite:
      return encodeFsWriteResult(FsWriteResult.ok(2));
    case MessageKind.FsPread:
      return encodeFsPreadResult(FsPreadResult.ok(ascii("hi")));
    case MessageKind.FsPwrite:
      return encodeFsPwriteResult(FsPwriteResult.ok());
    case MessageKind.FsLseek:
      return encodeFsLseekResult(FsLseekResult.ok(6));
    case MessageKind.FsFstat:
      return encodeFsFstatResult(FsFstatResult.ok(goldenFsStat()));
    case MessageKind.FsStat:
      return encodeFsStatResult(FsStatResult.ok(goldenFsStat()));
    case MessageKind.FsFsync:
      return encodeFsFsyncResult(FsFsyncResult.ok());
    case MessageKind.FsReaddir:
      return encodeFsReaddirResult(FsReaddirResult.ok(goldenDirents()));
    case MessageKind.RelationGet: {
      const row = new Array<Uint8Array | null>(2);
      row[0] = Uint8Array.wrap(String.UTF8.encode("\x0A"));
      row[1] = null;
      return encodeRelationGetResult(RelationGetResult.ok(row));
    }
    case MessageKind.RelationUpdate:
      return encodeRelationUpdateResult(RelationUpdateResult.ok(1));
    case MessageKind.RelationInsert:
      return encodeRelationInsertResult(RelationInsertResult.ok());
    default:
      throw new Error(`unknown response kind ${kind}`);
  }
}

// ---- ok response frames: decode and serialize to the sidecar expect shape ----

export function decodeResponseJson(kind: i32, frame: Uint8Array): string {
  switch (kind) {
    case MessageKind.Query: {
      const r = decodeQueryResult(frame);
      if (!r.isOk) throw new Error("expected ok query result");
      return queryResultJson(r.value);
    }
    case MessageKind.Command: {
      const r = decodeCommandResult(frame);
      if (!r.isOk) throw new Error("expected ok command result");
      return "{\"affected_rows\":\"" + r.value.affected_rows.toString() + "\"}";
    }
    case MessageKind.Batch: {
      const r = decodeBatchResult(frame);
      if (!r.isOk) throw new Error("expected ok batch result");
      return "{\"affected_rows\":\"" + r.value.affected_rows.toString() + "\"}";
    }
    case MessageKind.OpenSession: {
      const r = decodeOpenSessionResult(frame);
      if (!r.isOk) throw new Error("expected ok open-session result");
      // The sidecar renders the session UniOid as its u128 decimal value,
      // which equals l for every h == 0 session id used by the fixtures.
      if (r.value.h !== 0) {
        throw new Error("session oid serialization not covered by the sidecar conventions");
      }
      return "{\"session\":\"" + r.value.l.toString() + "\"}";
    }
    case MessageKind.CloseSession: {
      const r = decodeCloseSessionResult(frame);
      if (!r.isOk) throw new Error("expected ok close-session result");
      return UNIT_JSON;
    }
    case MessageKind.Get: {
      const r = decodeGetResult(frame);
      const v = r.value;
      if (!r.isOk) throw new Error("expected ok get result");
      if (v === null) {
        throw new Error("absent get value serialization not covered by the sidecar conventions");
      }
      return "{\"value_hex\":\"" + hexOf(v!) + "\"}";
    }
    case MessageKind.Put: {
      const r = decodePutResult(frame);
      if (!r.isOk) throw new Error("expected ok put result");
      return UNIT_JSON;
    }
    case MessageKind.Delete: {
      const r = decodeDeleteResult(frame);
      if (!r.isOk) throw new Error("expected ok delete result");
      return UNIT_JSON;
    }
    case MessageKind.Range: {
      const r = decodeRangeResult(frame);
      if (!r.isOk) throw new Error("expected ok range result");
      const items = new Array<string>(r.value.length);
      for (let i = 0; i < r.value.length; i++) {
        items[i] = "{\"key_hex\":\"" + hexOf(r.value[i].f0) + "\""
          + ",\"value_hex\":\"" + hexOf(r.value[i].f1) + "\"}";
      }
      return "{\"items\":[" + joinJson(items) + "]}";
    }
    case MessageKind.FsOpen: {
      const r = decodeFsOpenResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-open result");
      return "{\"fd\":" + r.value.toString() + "}";
    }
    case MessageKind.FsClose: {
      const r = decodeFsCloseResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-close result");
      return UNIT_JSON;
    }
    case MessageKind.FsRead: {
      const r = decodeFsReadResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-read result");
      return "{\"data_hex\":\"" + hexOf(r.value) + "\"}";
    }
    case MessageKind.FsWrite: {
      const r = decodeFsWriteResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-write result");
      return "{\"written\":" + r.value.toString() + "}";
    }
    case MessageKind.FsPread: {
      const r = decodeFsPreadResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-pread result");
      return "{\"data_hex\":\"" + hexOf(r.value) + "\"}";
    }
    case MessageKind.FsPwrite: {
      const r = decodeFsPwriteResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-pwrite result");
      return UNIT_JSON;
    }
    case MessageKind.FsLseek: {
      const r = decodeFsLseekResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-lseek result");
      return "{\"position\":\"" + r.value.toString() + "\"}";
    }
    case MessageKind.FsFstat: {
      const r = decodeFsFstatResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-fstat result");
      return "{\"stat\":" + fsStatJson(r.value) + "}";
    }
    case MessageKind.FsStat: {
      const r = decodeFsStatResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-stat result");
      return "{\"stat\":" + fsStatJson(r.value) + "}";
    }
    case MessageKind.FsFsync: {
      const r = decodeFsFsyncResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-fsync result");
      return UNIT_JSON;
    }
    case MessageKind.FsReaddir: {
      const r = decodeFsReaddirResult(frame);
      if (!r.isOk) throw new Error("expected ok fs-readdir result");
      return "{\"entries\":" + direntsJson(r.value) + "}";
    }
    case MessageKind.RelationGet: {
      const r = decodeRelationGetResult(frame);
      if (!r.isOk) throw new Error("expected ok relation-get result");
      return "{\"row\":" + relationRowJson(r.value) + "}";
    }
    case MessageKind.RelationUpdate: {
      const r = decodeRelationUpdateResult(frame);
      if (!r.isOk) throw new Error("expected ok relation-update result");
      return "{\"affected\":\"" + r.value.toString() + "\"}";
    }
    case MessageKind.RelationInsert: {
      const r = decodeRelationInsertResult(frame);
      if (!r.isOk) throw new Error("expected ok relation-insert result");
      return UNIT_JSON;
    }
    default:
      throw new Error(`unknown response kind ${kind}`);
  }
}

// ---- ok response frames: decode the corpus frame, then re-encode it ----

export function reencodeOkResponse(kind: i32, frame: Uint8Array): Uint8Array {
  switch (kind) {
    case MessageKind.Query: return encodeQueryResult(decodeQueryResult(frame));
    case MessageKind.Command: return encodeCommandResult(decodeCommandResult(frame));
    case MessageKind.Batch: return encodeBatchResult(decodeBatchResult(frame));
    case MessageKind.OpenSession: return encodeOpenSessionResult(decodeOpenSessionResult(frame));
    case MessageKind.CloseSession: return encodeCloseSessionResult(decodeCloseSessionResult(frame));
    case MessageKind.Get: return encodeGetResult(decodeGetResult(frame));
    case MessageKind.Put: return encodePutResult(decodePutResult(frame));
    case MessageKind.Delete: return encodeDeleteResult(decodeDeleteResult(frame));
    case MessageKind.Range: return encodeRangeResult(decodeRangeResult(frame));
    case MessageKind.FsOpen: return encodeFsOpenResult(decodeFsOpenResult(frame));
    case MessageKind.FsClose: return encodeFsCloseResult(decodeFsCloseResult(frame));
    case MessageKind.FsRead: return encodeFsReadResult(decodeFsReadResult(frame));
    case MessageKind.FsWrite: return encodeFsWriteResult(decodeFsWriteResult(frame));
    case MessageKind.FsPread: return encodeFsPreadResult(decodeFsPreadResult(frame));
    case MessageKind.FsPwrite: return encodeFsPwriteResult(decodeFsPwriteResult(frame));
    case MessageKind.FsLseek: return encodeFsLseekResult(decodeFsLseekResult(frame));
    case MessageKind.FsFstat: return encodeFsFstatResult(decodeFsFstatResult(frame));
    case MessageKind.FsStat: return encodeFsStatResult(decodeFsStatResult(frame));
    case MessageKind.FsFsync: return encodeFsFsyncResult(decodeFsFsyncResult(frame));
    case MessageKind.FsReaddir: return encodeFsReaddirResult(decodeFsReaddirResult(frame));
    case MessageKind.RelationGet: return encodeRelationGetResult(decodeRelationGetResult(frame));
    case MessageKind.RelationUpdate: return encodeRelationUpdateResult(decodeRelationUpdateResult(frame));
    case MessageKind.RelationInsert: return encodeRelationInsertResult(decodeRelationInsertResult(frame));
    default:
      throw new Error(`unknown response kind ${kind}`);
  }
}

// ---- trailing err frame (get result, NotFound) ----

export function encodeGetErrResponse(): Uint8Array {
  return encodeGetResult(GetResult.err(goldenErr()));
}

export function decodeGetErrJson(frame: Uint8Array): string {
  const r = decodeGetResult(frame);
  const err = r.error;
  if (r.isOk || err === null) throw new Error("expected err get result");
  return errJson(err!);
}

export function reencodeGetErrResponse(frame: Uint8Array): Uint8Array {
  return encodeGetResult(decodeGetResult(frame));
}

// ---- record/variant codec spot checks (compared in the runner against an
// independent JS MessagePack encoding) ----

// UniError with a non-empty err_details: pins the quirk that err_details
// encodes as a MessagePack ARRAY of u64, not bin.
export function encodeUniErrorSpot(): Uint8Array {
  const err = new UniError();
  err.err_code = 7;
  err.err_msg = "boom";
  err.err_src = "src";
  err.err_loc = "loc";
  err.err_details = Uint8Array.wrap(String.UTF8.encode("\x01\x02\x03"));
  const w = new MpackWriter();
  UniErrorCodec.encode(err, w);
  return w.toBytes();
}

export function verifyUniErrorSpot(bytes: Uint8Array): bool {
  const err = UniErrorCodec.decode(new MpackReader(bytes));
  return err.err_code === 7
    && err.err_msg === "boom"
    && err.err_src === "src"
    && err.err_loc === "loc"
    && bytesEq(err.err_details, Uint8Array.wrap(String.UTF8.encode("\x01\x02\x03")));
}

// UniResultSet with eof, one row (one U32 scalar field) and a non-empty
// cursor: pins the quirk that cursor encodes as a MessagePack ARRAY of u64.
export function encodeUniResultSetSpot(): Uint8Array {
  const scalar = new UniScalarValueU32();
  scalar.inner = 42;
  const dv = new UniDataValueScalar();
  dv.inner = scalar;
  const row = new UniTupleRow();
  row.fields = [dv];
  const rs = new UniResultSet();
  rs.eof = true;
  rs.row_set = [row];
  rs.cursor = Uint8Array.wrap(String.UTF8.encode("\x09\x08"));
  const w = new MpackWriter();
  UniResultSetCodec.encode(rs, w);
  return w.toBytes();
}

export function verifyUniResultSetSpot(bytes: Uint8Array): bool {
  const rs = UniResultSetCodec.decode(new MpackReader(bytes));
  if (!rs.eof || rs.row_set.length !== 1 || !bytesEq(rs.cursor, Uint8Array.wrap(String.UTF8.encode("\x09\x08")))) {
    return false;
  }
  const fields = rs.row_set[0].fields;
  if (fields.length !== 1) return false;
  const dv = fields[0];
  if (dv.kind !== 0) return false; // UniDataValueKind.Scalar
  const scalar = (dv as UniDataValueScalar).inner;
  return scalar.kind === 5 && (scalar as UniScalarValueU32).inner === 42;
}

// A single relation delta triple through the generated relation-update
// request encoder: [attr 3, op 0 (add), datum <05>].
export function encodeRelationDeltaSpot(): Uint8Array {
  return encodeRelationUpdateRequest(
    oid(0),
    "",
    new Array<RelationUpdateKeyItem>(0),
    new Array<RelationUpdateValuesItem>(0),
    relationUpdateDeltas(),
  );
}

// fs dirent record.
export function encodeFsDirentSpot(): Uint8Array {
  const d = new UniFsDirent();
  d.name = "a.txt";
  d.is_dir = false;
  d.length = 3;
  const w = new MpackWriter();
  UniFsDirentCodec.encode(d, w);
  return w.toBytes();
}

export function verifyFsDirentSpot(bytes: Uint8Array): bool {
  const d = UniFsDirentCodec.decode(new MpackReader(bytes));
  return d.name === "a.txt" && !d.is_dir && d.length === 3;
}
