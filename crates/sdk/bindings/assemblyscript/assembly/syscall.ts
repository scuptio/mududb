// MSSP (SyscallPayload v1) client over the host's `mududb:api/system`
// byte-pipe imports. Every import is a canonical-ABI `func(list<u8>) ->
// list<u8>`: the request goes in as a (ptr, len) pair and the response
// (ptr, len) pair is written to a guest-allocated 8-byte out area (the
// canonical-ABI shape for an imported function returning a list). Framing
// and uni wire shapes use the mgen-generated, corpus-verified codec in
// `generated/`.

import {
  decodeBatchResult,
  decodeCloseSessionResult,
  decodeCommandResult,
  decodeFsCloseResult,
  decodeFsFstatResult,
  decodeFsOpenResult,
  decodeFsPreadResult,
  decodeFsPwriteResult,
  decodeFsReadResult,
  decodeFsReaddirResult,
  decodeFsLseekResult,
  decodeFsStatResult,
  decodeFsFsyncResult,
  decodeFsWriteResult,
  decodeOpenSessionResult,
  decodeQueryResult,
  encodeBatchRequest,
  encodeCloseSessionRequest,
  encodeCommandRequest,
  encodeFsCloseRequest,
  encodeFsFstatRequest,
  encodeFsOpenRequest,
  encodeFsPreadRequest,
  encodeFsPwriteRequest,
  encodeFsReadRequest,
  encodeFsReaddirRequest,
  encodeFsLseekRequest,
  encodeFsStatRequest,
  encodeFsFsyncRequest,
  encodeFsWriteRequest,
  encodeOpenSessionRequest,
  encodeQueryRequest,
} from "./generated/UniSyscall";
import { UniCommandArgv } from "./generated/UniCommandArgv";
import { UniError } from "./generated/UniError";
import { UniFsDirent } from "./generated/UniFsDirent";
import { UniFsOpenArgv } from "./generated/UniFsOpenArgv";
import { UniFsStat } from "./generated/UniFsStat";
import { UniOid } from "./generated/UniOid";
import { UniQueryArgv } from "./generated/UniQueryArgv";
import { MuduError, Oid, alloc, bytesPtr, liftBytes } from "./wit";
import { Result, ResultSet } from "./result";
import { SqlStmt, ValueList } from "./sql";

@external("mududb:api/system", "query")
declare function hostQuery(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "command")
declare function hostCommand(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "batch")
declare function hostBatch(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "open")
declare function hostOpen(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "close")
declare function hostClose(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-open")
declare function hostFsOpen(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-close")
declare function hostFsClose(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-read")
declare function hostFsRead(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-write")
declare function hostFsWrite(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-pread")
declare function hostFsPread(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-pwrite")
declare function hostFsPwrite(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-lseek")
declare function hostFsLseek(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-fstat")
declare function hostFsFstat(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-stat")
declare function hostFsStat(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-fsync")
declare function hostFsFsync(ptr: usize, len: usize, resultPtr: usize): void;
@external("mududb:api/system", "fs-readdir")
declare function hostFsReaddir(ptr: usize, len: usize, resultPtr: usize): void;

export function uniOid(id: Oid): UniOid {
  const oid = new UniOid();
  oid.h = id.hi;
  oid.l = id.lo;
  return oid;
}

export function muduErrorFromUni(error: UniError): MuduError {
  return new MuduError(error.err_code, error.err_msg, error.err_src, error.err_loc);
}

function throwUniError(error: UniError): void {
  throw new Error(error.err_msg);
}

// After the import returns, the 8-byte out area holds the (ptr, len) pair the
// canonical-ABI trampoline placed in guest memory; copy the payload out.
function liftResponse(out: usize): Uint8Array {
  const dataPtr = load<u32>(out) as usize;
  const dataLen = load<u32>(out + 4) as usize;
  return liftBytes(dataPtr, dataLen);
}

export function sysQueryBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostQuery(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysCommandBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostCommand(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysBatchBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostBatch(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysOpenBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostOpen(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysCloseBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostClose(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsOpenBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsOpen(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsCloseBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsClose(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsReadBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsRead(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsWriteBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsWrite(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsPreadBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsPread(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsPwriteBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsPwrite(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsLseekBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsLseek(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsFstatBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsFstat(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsStatBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsStat(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsFsyncBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsFsync(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

export function sysFsReaddirBytes(request: Uint8Array): Uint8Array {
  const out = alloc(8);
  hostFsReaddir(bytesPtr(request), request.length, out);
  return liftResponse(out);
}

function fillSqlArgv(argv: UniQueryArgv, id: Oid, sql: string, values: ValueList): void {
  argv.oid = uniOid(id);
  argv.query.sql_string = sql;
  argv.param_list = values.toUniSqlParam();
  // `param-desc` stays null: the host skips the type check then (same as the
  // C# guest).
}

export function witQuery(id: Oid, stmt: SqlStmt, values: ValueList): ResultSet {
  const argv = new UniQueryArgv();
  fillSqlArgv(argv, id, stmt.sql, values);
  const result = decodeQueryResult(sysQueryBytes(encodeQueryRequest(argv)));
  const error = result.error;
  if (error !== null) {
    throwUniError(error);
  }
  return new ResultSet(result.value);
}

function invokeCommand(id: Oid, stmt: SqlStmt, values: ValueList, batch: bool): u64 {
  const argv = new UniCommandArgv();
  argv.oid = uniOid(id);
  argv.command.sql_string = stmt.sql;
  argv.param_list = values.toUniSqlParam();
  if (batch) {
    const result = decodeBatchResult(sysBatchBytes(encodeBatchRequest(argv)));
    const error = result.error;
    if (error !== null) {
      throwUniError(error);
    }
    return result.value.affected_rows;
  }
  const result = decodeCommandResult(sysCommandBytes(encodeCommandRequest(argv)));
  const error = result.error;
  if (error !== null) {
    throwUniError(error);
  }
  return result.value.affected_rows;
}

export function witCommand(id: Oid, stmt: SqlStmt, values: ValueList): u64 {
  return invokeCommand(id, stmt, values, false);
}

export function witBatch(id: Oid, stmt: SqlStmt, values: ValueList): u64 {
  return invokeCommand(id, stmt, values, true);
}

export function witOpen(uri: string): Oid {
  const result = decodeOpenSessionResult(sysOpenBytes(encodeOpenSessionRequest(parseWorkerOid(uri))));
  const error = result.error;
  if (error !== null) {
    throwUniError(error);
  }
  return new Oid(result.value.h, result.value.l);
}

export function witClose(id: Oid): void {
  const result = decodeCloseSessionResult(sysCloseBytes(encodeCloseSessionRequest(uniOid(id))));
  const error = result.error;
  if (error !== null) {
    throwUniError(error);
  }
}

// The open "uri" is either empty (the default worker) or a decimal u128
// worker object id.
function parseWorkerOid(uri: string): UniOid {
  const oid = new UniOid();
  if (uri.length == 0) {
    return oid;
  }
  let hi: u64 = 0;
  let lo: u64 = 0;
  for (let i = 0; i < uri.length; i++) {
    const code = uri.charCodeAt(i);
    if (code < 48 || code > 57) {
      throw new Error("open uri must be empty or a numeric worker object id");
    }
    const digit = (code - 48) as u64;
    // (hi, lo) = (hi, lo) * 10 + digit, 32-bit limb arithmetic with carry.
    const lo0 = lo & 0xffffffff;
    const lo1 = lo >> 32;
    const hi0 = hi & 0xffffffff;
    const hi1 = hi >> 32;
    const t0 = lo0 * 10 + digit;
    const t1 = lo1 * 10 + (t0 >> 32);
    const t2 = hi0 * 10 + (t1 >> 32);
    const t3 = hi1 * 10 + (t2 >> 32);
    if ((t3 >> 32) != 0) {
      throw new Error("worker object id does not fit into u128");
    }
    lo = ((t1 & 0xffffffff) << 32) | (t0 & 0xffffffff);
    hi = ((t3 & 0xffffffff) << 32) | (t2 & 0xffffffff);
  }
  oid.h = hi;
  oid.l = lo;
  return oid;
}

// ---- fs syscall wrappers (used by fs.ts) ----

export function sysFsOpen(session: Oid, oid: Oid, path: string, flags: u32): Result<u32> {
  const argv = new UniFsOpenArgv();
  argv.session = uniOid(session);
  argv.oid = uniOid(oid);
  argv.path = path;
  argv.flags = flags;
  const result = decodeFsOpenResult(sysFsOpenBytes(encodeFsOpenRequest(argv)));
  const error = result.error;
  return error === null
    ? Result.ok<u32>(result.value)
    : Result.error<u32>(muduErrorFromUni(error));
}

export function sysFsClose(fd: u32): Result<bool> {
  const result = decodeFsCloseResult(sysFsCloseBytes(encodeFsCloseRequest(fd)));
  const error = result.error;
  return error === null ? Result.ok<bool>(true) : Result.error<bool>(muduErrorFromUni(error));
}

export function sysFsRead(fd: u32, len: u32): Result<Uint8Array> {
  const result = decodeFsReadResult(sysFsReadBytes(encodeFsReadRequest(fd, len)));
  const error = result.error;
  return error === null
    ? Result.ok<Uint8Array>(result.value)
    : Result.error<Uint8Array>(muduErrorFromUni(error));
}

export function sysFsWrite(fd: u32, data: Uint8Array): Result<u32> {
  const result = decodeFsWriteResult(sysFsWriteBytes(encodeFsWriteRequest(fd, data)));
  const error = result.error;
  return error === null
    ? Result.ok<u32>(result.value)
    : Result.error<u32>(muduErrorFromUni(error));
}

export function sysFsPread(fd: u32, offset: u64, len: u32): Result<Uint8Array> {
  const result = decodeFsPreadResult(sysFsPreadBytes(encodeFsPreadRequest(fd, offset, len)));
  const error = result.error;
  return error === null
    ? Result.ok<Uint8Array>(result.value)
    : Result.error<Uint8Array>(muduErrorFromUni(error));
}

export function sysFsPwrite(fd: u32, offset: u64, data: Uint8Array): Result<bool> {
  const result = decodeFsPwriteResult(sysFsPwriteBytes(encodeFsPwriteRequest(fd, offset, data)));
  const error = result.error;
  return error === null ? Result.ok<bool>(true) : Result.error<bool>(muduErrorFromUni(error));
}

export function sysFsLseek(fd: u32, offset: i64, whence: u32): Result<u64> {
  const result = decodeFsLseekResult(sysFsLseekBytes(encodeFsLseekRequest(fd, offset, whence)));
  const error = result.error;
  return error === null
    ? Result.ok<u64>(result.value)
    : Result.error<u64>(muduErrorFromUni(error));
}

export function sysFsFstat(fd: u32): Result<UniFsStat> {
  const result = decodeFsFstatResult(sysFsFstatBytes(encodeFsFstatRequest(fd)));
  const error = result.error;
  return error === null
    ? Result.ok<UniFsStat>(result.value)
    : Result.error<UniFsStat>(muduErrorFromUni(error));
}

export function sysFsStat(oid: Oid, path: string): Result<UniFsStat> {
  const result = decodeFsStatResult(sysFsStatBytes(encodeFsStatRequest(uniOid(oid), path)));
  const error = result.error;
  return error === null
    ? Result.ok<UniFsStat>(result.value)
    : Result.error<UniFsStat>(muduErrorFromUni(error));
}

export function sysFsFsync(fd: u32): Result<bool> {
  const result = decodeFsFsyncResult(sysFsFsyncBytes(encodeFsFsyncRequest(fd)));
  const error = result.error;
  return error === null ? Result.ok<bool>(true) : Result.error<bool>(muduErrorFromUni(error));
}

export function sysFsReaddir(oid: Oid, path: string): Result<Array<UniFsDirent>> {
  const result = decodeFsReaddirResult(sysFsReaddirBytes(encodeFsReaddirRequest(uniOid(oid), path)));
  const error = result.error;
  return error === null
    ? Result.ok<Array<UniFsDirent>>(result.value)
    : Result.error<Array<UniFsDirent>>(muduErrorFromUni(error));
}
