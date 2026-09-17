import * as __import0 from "mududb:api/system";
async function instantiate(module, imports = {}) {
  const __module0 = imports["mududb:api/system"];
  const adaptedImports = {
    env: Object.assign(Object.create(globalThis), imports.env || {}, {
      abort(message, fileName, lineNumber, columnNumber) {
        // ~lib/builtins/abort(~lib/string/String | null?, ~lib/string/String | null?, u32?, u32?) => void
        message = __liftString(message >>> 0);
        fileName = __liftString(fileName >>> 0);
        lineNumber = lineNumber >>> 0;
        columnNumber = columnNumber >>> 0;
        (() => {
          // @external.js
          throw Error(`${message} in ${fileName}:${lineNumber}:${columnNumber}`);
        })();
      },
    }),
    "mududb:api/system": Object.assign(Object.create(__module0), {
      "fs-close"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsClose(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-close(ptr, len, resultPtr);
      },
      "fs-fstat"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsFstat(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-fstat(ptr, len, resultPtr);
      },
      "fs-fsync"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsFsync(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-fsync(ptr, len, resultPtr);
      },
      "fs-lseek"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsLseek(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-lseek(ptr, len, resultPtr);
      },
      "fs-open"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsOpen(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-open(ptr, len, resultPtr);
      },
      "fs-pread"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsPread(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-pread(ptr, len, resultPtr);
      },
      "fs-pwrite"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsPwrite(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-pwrite(ptr, len, resultPtr);
      },
      "fs-read"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsRead(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-read(ptr, len, resultPtr);
      },
      "fs-readdir"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsReaddir(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-readdir(ptr, len, resultPtr);
      },
      "fs-stat"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsStat(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-stat(ptr, len, resultPtr);
      },
      "fs-write"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsWrite(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-write(ptr, len, resultPtr);
      },
      batch(ptr, len, resultPtr) {
        // assembly/syscall/hostBatch(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.batch(ptr, len, resultPtr);
      },
      command(ptr, len, resultPtr) {
        // assembly/syscall/hostCommand(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.command(ptr, len, resultPtr);
      },
      close(ptr, len, resultPtr) {
        // assembly/syscall/hostClose(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.close(ptr, len, resultPtr);
      },
      open(ptr, len, resultPtr) {
        // assembly/syscall/hostOpen(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.open(ptr, len, resultPtr);
      },
      query(ptr, len, resultPtr) {
        // assembly/syscall/hostQuery(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.query(ptr, len, resultPtr);
      },
    }),
  };
  const { exports } = await WebAssembly.instantiate(module, adaptedImports);
  const memory = exports.memory || imports.env.memory;
  const adaptedExports = Object.setPrototypeOf({
    FS_O_RDONLY: {
      // assembly/fs/FS_O_RDONLY: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_O_RDONLY.value >>> 0;
      }
    },
    FS_O_RDWR: {
      // assembly/fs/FS_O_RDWR: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_O_RDWR.value >>> 0;
      }
    },
    FS_O_WRONLY: {
      // assembly/fs/FS_O_WRONLY: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_O_WRONLY.value >>> 0;
      }
    },
    FS_SEEK_CUR: {
      // assembly/fs/FS_SEEK_CUR: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_SEEK_CUR.value >>> 0;
      }
    },
    FS_SEEK_END: {
      // assembly/fs/FS_SEEK_END: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_SEEK_END.value >>> 0;
      }
    },
    FS_SEEK_SET: {
      // assembly/fs/FS_SEEK_SET: u32
      valueOf() { return this.value; },
      get value() {
        return exports.FS_SEEK_SET.value >>> 0;
      }
    },
    fsClose(sessionHi, sessionLo, fd) {
      // assembly/fs/fsClose(u64, u64, u32) => assembly/result/Result<bool>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      return __liftInternref(exports.fsClose(sessionHi, sessionLo, fd) >>> 0);
    },
    fsFstat(sessionHi, sessionLo, fd) {
      // assembly/fs/fsFstat(u64, u64, u32) => assembly/result/Result<assembly/fs/FsStat>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      return __liftInternref(exports.fsFstat(sessionHi, sessionLo, fd) >>> 0);
    },
    fsFsync(sessionHi, sessionLo, fd) {
      // assembly/fs/fsFsync(u64, u64, u32) => assembly/result/Result<bool>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      return __liftInternref(exports.fsFsync(sessionHi, sessionLo, fd) >>> 0);
    },
    fsLseek(sessionHi, sessionLo, fd, offset, whence) {
      // assembly/fs/fsLseek(u64, u64, u32, i64, u32) => assembly/result/Result<u64>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      offset = offset || 0n;
      return __liftInternref(exports.fsLseek(sessionHi, sessionLo, fd, offset, whence) >>> 0);
    },
    fsOpen(sessionHi, sessionLo, oidHi, oidLo, path, flags) {
      // assembly/fs/fsOpen(u64, u64, u64, u64, ~lib/string/String, u32) => assembly/result/Result<u32>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      oidHi = oidHi || 0n;
      oidLo = oidLo || 0n;
      path = __lowerString(path) || __notnull();
      return __liftInternref(exports.fsOpen(sessionHi, sessionLo, oidHi, oidLo, path, flags) >>> 0);
    },
    fsPread(sessionHi, sessionLo, fd, offset, len) {
      // assembly/fs/fsPread(u64, u64, u32, u64, u32) => assembly/result/Result<~lib/arraybuffer/ArrayBuffer>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      offset = offset || 0n;
      return __liftInternref(exports.fsPread(sessionHi, sessionLo, fd, offset, len) >>> 0);
    },
    fsPwrite(sessionHi, sessionLo, fd, offset, data) {
      // assembly/fs/fsPwrite(u64, u64, u32, u64, ~lib/arraybuffer/ArrayBuffer) => assembly/result/Result<bool>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      offset = offset || 0n;
      data = __lowerBuffer(data) || __notnull();
      return __liftInternref(exports.fsPwrite(sessionHi, sessionLo, fd, offset, data) >>> 0);
    },
    fsRead(sessionHi, sessionLo, fd, len) {
      // assembly/fs/fsRead(u64, u64, u32, u32) => assembly/result/Result<~lib/arraybuffer/ArrayBuffer>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      return __liftInternref(exports.fsRead(sessionHi, sessionLo, fd, len) >>> 0);
    },
    fsReaddir(sessionHi, sessionLo, oidHi, oidLo, path) {
      // assembly/fs/fsReaddir(u64, u64, u64, u64, ~lib/string/String) => assembly/result/Result<~lib/array/Array<assembly/fs/FsDirEntry>>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      oidHi = oidHi || 0n;
      oidLo = oidLo || 0n;
      path = __lowerString(path) || __notnull();
      return __liftInternref(exports.fsReaddir(sessionHi, sessionLo, oidHi, oidLo, path) >>> 0);
    },
    fsStat(sessionHi, sessionLo, oidHi, oidLo, path) {
      // assembly/fs/fsStat(u64, u64, u64, u64, ~lib/string/String) => assembly/result/Result<assembly/fs/FsStat>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      oidHi = oidHi || 0n;
      oidLo = oidLo || 0n;
      path = __lowerString(path) || __notnull();
      return __liftInternref(exports.fsStat(sessionHi, sessionLo, oidHi, oidLo, path) >>> 0);
    },
    fsWrite(sessionHi, sessionLo, fd, data) {
      // assembly/fs/fsWrite(u64, u64, u32, ~lib/arraybuffer/ArrayBuffer) => assembly/result/Result<u32>
      sessionHi = sessionHi || 0n;
      sessionLo = sessionLo || 0n;
      data = __lowerBuffer(data) || __notnull();
      return __liftInternref(exports.fsWrite(sessionHi, sessionLo, fd, data) >>> 0);
    },
    decodeProcedureParam(paramPtr, paramLen) {
      // assembly/procedure/decodeProcedureParam(usize, usize) => assembly/generated/UniProcedureParam/UniProcedureParam
      return __liftRecord40(exports.decodeProcedureParam(paramPtr, paramLen) >>> 0);
    },
    encodeProcedureErr(error, procedure) {
      // assembly/procedure/encodeProcedureErr(assembly/wit/MuduError, ~lib/string/String) => usize
      error = __retain(__lowerInternref(error) || __notnull());
      procedure = __lowerString(procedure) || __notnull();
      try {
        return exports.encodeProcedureErr(error, procedure) >>> 0;
      } finally {
        __release(error);
      }
    },
    encodeProcedureOk(values) {
      // assembly/procedure/encodeProcedureOk(assembly/sql/ValueList) => usize
      values = __lowerInternref(values) || __notnull();
      return exports.encodeProcedureOk(values) >>> 0;
    },
    encodeProcedureOkUni(values) {
      // assembly/procedure/encodeProcedureOkUni(~lib/array/Array<assembly/generated/UniDataValue/UniDataValue>) => usize
      values = __lowerArray((pointer, value) => { __setU32(pointer, __lowerRecord41(value) || __notnull()); }, 42, 2, values) || __notnull();
      return exports.encodeProcedureOkUni(values) >>> 0;
    },
    isNullDatum(value) {
      // assembly/record/isNullDatum(assembly/generated/UniDataValue/UniDataValue) => bool
      value = __lowerRecord41(value) || __notnull();
      return exports.isNullDatum(value) != 0;
    },
    recordFieldValues(value) {
      // assembly/record/recordFieldValues(assembly/generated/UniDataValue/UniDataValue) => ~lib/typedarray/Uint8Array
      value = __lowerRecord41(value) || __notnull();
      return __liftTypedArray(Uint8Array, exports.recordFieldValues(value) >>> 0);
    },
    recordFromFieldValues(fieldMapBytes) {
      // assembly/record/recordFromFieldValues(~lib/typedarray/Uint8Array) => assembly/generated/UniDataValue/UniDataValue
      fieldMapBytes = __lowerTypedArray(Uint8Array, 6, 0, fieldMapBytes) || __notnull();
      return __liftRecord41(exports.recordFromFieldValues(fieldMapBytes) >>> 0);
    },
    procedureResultErr(error, procedure, location) {
      // assembly/result/procedureResultErr(assembly/wit/MuduError, ~lib/string/String?, ~lib/string/String?) => assembly/result/Result<assembly/sql/ValueList>
      error = __retain(__lowerInternref(error) || __notnull());
      procedure = __retain(__lowerString(procedure) || __notnull());
      location = __lowerString(location) || __notnull();
      try {
        exports.__setArgumentsLength(arguments.length);
        return __liftInternref(exports.procedureResultErr(error, procedure, location) >>> 0);
      } finally {
        __release(error);
        __release(procedure);
      }
    },
    procedureResultOk(values) {
      // assembly/result/procedureResultOk(assembly/sql/ValueList) => assembly/result/Result<assembly/sql/ValueList>
      values = __lowerInternref(values) || __notnull();
      return __liftInternref(exports.procedureResultOk(values) >>> 0);
    },
    witBatch(id, stmt, values) {
      // assembly/syscall/witBatch(assembly/wit/Oid, assembly/sql/SqlStmt, assembly/sql/ValueList) => u64
      id = __retain(__lowerInternref(id) || __notnull());
      stmt = __retain(__lowerInternref(stmt) || __notnull());
      values = __lowerInternref(values) || __notnull();
      try {
        return BigInt.asUintN(64, exports.witBatch(id, stmt, values));
      } finally {
        __release(id);
        __release(stmt);
      }
    },
    witClose(id) {
      // assembly/syscall/witClose(assembly/wit/Oid) => void
      id = __lowerInternref(id) || __notnull();
      exports.witClose(id);
    },
    witCommand(id, stmt, values) {
      // assembly/syscall/witCommand(assembly/wit/Oid, assembly/sql/SqlStmt, assembly/sql/ValueList) => u64
      id = __retain(__lowerInternref(id) || __notnull());
      stmt = __retain(__lowerInternref(stmt) || __notnull());
      values = __lowerInternref(values) || __notnull();
      try {
        return BigInt.asUintN(64, exports.witCommand(id, stmt, values));
      } finally {
        __release(id);
        __release(stmt);
      }
    },
    witOpen(uri) {
      // assembly/syscall/witOpen(~lib/string/String) => assembly/wit/Oid
      uri = __lowerString(uri) || __notnull();
      return __liftInternref(exports.witOpen(uri) >>> 0);
    },
    witQuery(id, stmt, values) {
      // assembly/syscall/witQuery(assembly/wit/Oid, assembly/sql/SqlStmt, assembly/sql/ValueList) => assembly/result/ResultSet
      id = __retain(__lowerInternref(id) || __notnull());
      stmt = __retain(__lowerInternref(stmt) || __notnull());
      values = __lowerInternref(values) || __notnull();
      try {
        return __liftInternref(exports.witQuery(id, stmt, values) >>> 0);
      } finally {
        __release(id);
        __release(stmt);
      }
    },
    ERROR_DOMAIN_VIOLATION: {
      // assembly/wit/ERROR_DOMAIN_VIOLATION: u32
      valueOf() { return this.value; },
      get value() {
        return exports.ERROR_DOMAIN_VIOLATION.value >>> 0;
      }
    },
    ERROR_ENTITY_NOT_FOUND: {
      // assembly/wit/ERROR_ENTITY_NOT_FOUND: u32
      valueOf() { return this.value; },
      get value() {
        return exports.ERROR_ENTITY_NOT_FOUND.value >>> 0;
      }
    },
    ERROR_INTERNAL: {
      // assembly/wit/ERROR_INTERNAL: u32
      valueOf() { return this.value; },
      get value() {
        return exports.ERROR_INTERNAL.value >>> 0;
      }
    },
    ERROR_INVALID_ARGUMENT: {
      // assembly/wit/ERROR_INVALID_ARGUMENT: u32
      valueOf() { return this.value; },
      get value() {
        return exports.ERROR_INVALID_ARGUMENT.value >>> 0;
      }
    },
    ValueKind: (values => (
      // assembly/wit/ValueKind
      values[values.Null = exports["ValueKind.Null"].valueOf()] = "Null",
      values[values.Boolean = exports["ValueKind.Boolean"].valueOf()] = "Boolean",
      values[values.Int64 = exports["ValueKind.Int64"].valueOf()] = "Int64",
      values[values.Float64 = exports["ValueKind.Float64"].valueOf()] = "Float64",
      values[values.Text = exports["ValueKind.Text"].valueOf()] = "Text",
      values[values.Binary = exports["ValueKind.Binary"].valueOf()] = "Binary",
      values[values.ObjectId = exports["ValueKind.ObjectId"].valueOf()] = "ObjectId",
      values
    ))({}),
    cabi_realloc(oldPtr, oldSize, align, newSize) {
      // assembly/wit/cabi_realloc(usize, usize, usize, usize) => usize
      return exports.cabi_realloc(oldPtr, oldSize, align, newSize) >>> 0;
    },
  }, exports);
  function __liftRecord16(pointer) {
    // assembly/generated/UniOid/UniOid
    // Hint: Opt-out from lifting as a record by providing an empty constructor
    if (!pointer) return null;
    return {
      h: __getU64(pointer + 0),
      l: __getU64(pointer + 8),
    };
  }
  function __liftRecord41(pointer) {
    // assembly/generated/UniDataValue/UniDataValue
    // Hint: Opt-out from lifting as a record by providing an empty constructor
    if (!pointer) return null;
    return {
      kind: __getI32(pointer + 0),
    };
  }
  function __liftRecord40(pointer) {
    // assembly/generated/UniProcedureParam/UniProcedureParam
    // Hint: Opt-out from lifting as a record by providing an empty constructor
    if (!pointer) return null;
    return {
      procedure: __getU64(pointer + 0),
      session: __liftRecord16(__getU32(pointer + 8)),
      param_list: __liftArray(pointer => __liftRecord41(__getU32(pointer)), 2, __getU32(pointer + 12)),
    };
  }
  function __lowerRecord41(value) {
    // assembly/generated/UniDataValue/UniDataValue
    // Hint: Opt-out from lowering as a record by providing an empty constructor
    if (value == null) return 0;
    const pointer = exports.__pin(exports.__new(4, 41));
    __setU32(pointer + 0, value.kind);
    exports.__unpin(pointer);
    return pointer;
  }
  function __lowerBuffer(value) {
    if (value == null) return 0;
    const pointer = exports.__new(value.byteLength, 1) >>> 0;
    new Uint8Array(memory.buffer).set(new Uint8Array(value), pointer);
    return pointer;
  }
  function __liftString(pointer) {
    if (!pointer) return null;
    const
      end = pointer + new Uint32Array(memory.buffer)[pointer - 4 >>> 2] >>> 1,
      memoryU16 = new Uint16Array(memory.buffer);
    let
      start = pointer >>> 1,
      string = "";
    while (end - start > 1024) string += String.fromCharCode(...memoryU16.subarray(start, start += 1024));
    return string + String.fromCharCode(...memoryU16.subarray(start, end));
  }
  function __lowerString(value) {
    if (value == null) return 0;
    const
      length = value.length,
      pointer = exports.__new(length << 1, 2) >>> 0,
      memoryU16 = new Uint16Array(memory.buffer);
    for (let i = 0; i < length; ++i) memoryU16[(pointer >>> 1) + i] = value.charCodeAt(i);
    return pointer;
  }
  function __liftArray(liftElement, align, pointer) {
    if (!pointer) return null;
    const
      dataStart = __getU32(pointer + 4),
      length = __dataview.getUint32(pointer + 12, true),
      values = new Array(length);
    for (let i = 0; i < length; ++i) values[i] = liftElement(dataStart + (i << align >>> 0));
    return values;
  }
  function __lowerArray(lowerElement, id, align, values) {
    if (values == null) return 0;
    const
      length = values.length,
      buffer = exports.__pin(exports.__new(length << align, 1)) >>> 0,
      header = exports.__pin(exports.__new(16, id)) >>> 0;
    __setU32(header + 0, buffer);
    __dataview.setUint32(header + 4, buffer, true);
    __dataview.setUint32(header + 8, length << align, true);
    __dataview.setUint32(header + 12, length, true);
    for (let i = 0; i < length; ++i) lowerElement(buffer + (i << align >>> 0), values[i]);
    exports.__unpin(buffer);
    exports.__unpin(header);
    return header;
  }
  function __liftTypedArray(constructor, pointer) {
    if (!pointer) return null;
    return new constructor(
      memory.buffer,
      __getU32(pointer + 4),
      __dataview.getUint32(pointer + 8, true) / constructor.BYTES_PER_ELEMENT
    ).slice();
  }
  function __lowerTypedArray(constructor, id, align, values) {
    if (values == null) return 0;
    const
      length = values.length,
      buffer = exports.__pin(exports.__new(length << align, 1)) >>> 0,
      header = exports.__new(12, id) >>> 0;
    __setU32(header + 0, buffer);
    __dataview.setUint32(header + 4, buffer, true);
    __dataview.setUint32(header + 8, length << align, true);
    new constructor(memory.buffer, buffer, length).set(values);
    exports.__unpin(buffer);
    return header;
  }
  class Internref extends Number {}
  const registry = new FinalizationRegistry(__release);
  function __liftInternref(pointer) {
    if (!pointer) return null;
    const sentinel = new Internref(__retain(pointer));
    registry.register(sentinel, pointer);
    return sentinel;
  }
  function __lowerInternref(value) {
    if (value == null) return 0;
    if (value instanceof Internref) return value.valueOf();
    throw TypeError("internref expected");
  }
  const refcounts = new Map();
  function __retain(pointer) {
    if (pointer) {
      const refcount = refcounts.get(pointer);
      if (refcount) refcounts.set(pointer, refcount + 1);
      else refcounts.set(exports.__pin(pointer), 1);
    }
    return pointer;
  }
  function __release(pointer) {
    if (pointer) {
      const refcount = refcounts.get(pointer);
      if (refcount === 1) exports.__unpin(pointer), refcounts.delete(pointer);
      else if (refcount) refcounts.set(pointer, refcount - 1);
      else throw Error(`invalid refcount '${refcount}' for reference '${pointer}'`);
    }
  }
  function __notnull() {
    throw TypeError("value must not be null");
  }
  let __dataview = new DataView(memory.buffer);
  function __setU32(pointer, value) {
    try {
      __dataview.setUint32(pointer, value, true);
    } catch {
      __dataview = new DataView(memory.buffer);
      __dataview.setUint32(pointer, value, true);
    }
  }
  function __getI32(pointer) {
    try {
      return __dataview.getInt32(pointer, true);
    } catch {
      __dataview = new DataView(memory.buffer);
      return __dataview.getInt32(pointer, true);
    }
  }
  function __getU32(pointer) {
    try {
      return __dataview.getUint32(pointer, true);
    } catch {
      __dataview = new DataView(memory.buffer);
      return __dataview.getUint32(pointer, true);
    }
  }
  function __getU64(pointer) {
    try {
      return __dataview.getBigUint64(pointer, true);
    } catch {
      __dataview = new DataView(memory.buffer);
      return __dataview.getBigUint64(pointer, true);
    }
  }
  return adaptedExports;
}
export const {
  memory,
  FS_O_RDONLY,
  FS_O_RDWR,
  FS_O_WRONLY,
  FS_SEEK_CUR,
  FS_SEEK_END,
  FS_SEEK_SET,
  fsClose,
  fsFstat,
  fsFsync,
  fsLseek,
  fsOpen,
  fsPread,
  fsPwrite,
  fsRead,
  fsReaddir,
  fsStat,
  fsWrite,
  decodeProcedureParam,
  encodeProcedureErr,
  encodeProcedureOk,
  encodeProcedureOkUni,
  isNullDatum,
  recordFieldValues,
  recordFromFieldValues,
  procedureResultErr,
  procedureResultOk,
  witBatch,
  witClose,
  witCommand,
  witOpen,
  witQuery,
  ERROR_DOMAIN_VIOLATION,
  ERROR_ENTITY_NOT_FOUND,
  ERROR_INTERNAL,
  ERROR_INVALID_ARGUMENT,
  ValueKind,
  cabi_realloc,
} = await (async url => instantiate(
  await (async () => {
    const isNodeOrBun = typeof process != "undefined" && process.versions != null && (process.versions.node != null || process.versions.bun != null);
    if (isNodeOrBun) { return globalThis.WebAssembly.compile(await (await import("node:fs/promises")).readFile(url)); }
    else { return await globalThis.WebAssembly.compileStreaming(globalThis.fetch(url)); }
  })(), {
    "mududb:api/system": __maybeDefault(__import0),
  }
))(new URL("mududb-as.wasm", import.meta.url));
function __maybeDefault(module) {
  return typeof module.default === "object" && Object.keys(module).length == 1
    ? module.default
    : module;
}
