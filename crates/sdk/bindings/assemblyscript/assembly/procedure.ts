// mp2 byte-pipe helpers for the mtp-generated procedure adapters: decode the
// `UniProcedureParam` the runtime passes to each `mp2-<proc>` export and
// encode the `UniResult<UniProcedureResult, UniError>` it expects back (a
// single-entry MessagePack map: key 0 = ok, key 1 = `UniError`).

import { MpackReader, MpackWriter } from "./mpack";
import { UniDataValue } from "./generated/UniDataValue";
import { UniError, UniErrorCodec } from "./generated/UniError";
import { UniProcedureParam, UniProcedureParamCodec } from "./generated/UniProcedureParam";
import { UniProcedureResult, UniProcedureResultCodec } from "./generated/UniProcedureResult";
import { ERROR_INTERNAL, MuduError, alloc, liftBytes } from "./wit";
import { ValueList } from "./sql";

// Decode the canonical-ABI argument: `paramLen` MessagePack bytes at
// `paramPtr`, placed in guest memory by the runtime through `cabi_realloc`.
export function decodeProcedureParam(paramPtr: usize, paramLen: usize): UniProcedureParam {
  const bytes = liftBytes(paramPtr, paramLen);
  const reader = new MpackReader(bytes);
  return UniProcedureParamCodec.decode(reader);
}

// Ok arm: `{0: {1: [return_list]}}`.
export function encodeProcedureOk(values: ValueList): usize {
  return encodeProcedureOkUni(values.toUniDataValues());
}

// Ok arm over raw wire datums: the shape record-typed results take (a
// `ValueList` carries only the scalar DX value kinds, so a record result is
// wrapped by `recordFromFieldValues` and passed here directly).
export function encodeProcedureOkUni(values: Array<UniDataValue>): usize {
  const result = new UniProcedureResult();
  result.return_list = values;
  const writer = new MpackWriter();
  writer.writeMapHeader(1);
  writer.writeU64(0);
  UniProcedureResultCodec.encode(result, writer);
  return returnBytes(writer.toBytes());
}

// Err arm: `{1: UniError}`; the error source defaults to "assemblyscript" and
// the error location to the procedure name (mirroring `procedureResultErr`).
export function encodeProcedureErr(error: MuduError, procedure: string): usize {
  const uniError = new UniError();
  uniError.err_code = error.code == 0 ? ERROR_INTERNAL : error.code;
  uniError.err_msg = error.message;
  uniError.err_src = error.source.length > 0 ? error.source : "assemblyscript";
  uniError.err_loc = error.location.length > 0 ? error.location : procedure;
  const writer = new MpackWriter();
  writer.writeMapHeader(1);
  writer.writeU64(1);
  UniErrorCodec.encode(uniError, writer);
  return returnBytes(writer.toBytes());
}

// A `func(...) -> list<u8>` export returns its payload through a pointer to a
// (ptr, len) pair in guest memory.
function returnBytes(bytes: Uint8Array): usize {
  const pair = alloc(8);
  store<u32>(pair, <u32>(changetype<usize>(bytes.buffer) + bytes.byteOffset));
  store<u32>(pair + 4, <u32>bytes.length);
  return pair;
}
