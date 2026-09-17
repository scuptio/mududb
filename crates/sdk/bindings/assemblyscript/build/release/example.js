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
      open(ptr, len, resultPtr) {
        // assembly/syscall/hostOpen(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.open(ptr, len, resultPtr);
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
      query(ptr, len, resultPtr) {
        // assembly/syscall/hostQuery(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.query(ptr, len, resultPtr);
      },
      close(ptr, len, resultPtr) {
        // assembly/syscall/hostClose(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.close(ptr, len, resultPtr);
      },
      "fs-open"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsOpen(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-open(ptr, len, resultPtr);
      },
      "fs-write"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsWrite(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-write(ptr, len, resultPtr);
      },
      "fs-close"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsClose(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-close(ptr, len, resultPtr);
      },
      "fs-read"(ptr, len, resultPtr) {
        // assembly/syscall/hostFsRead(usize, usize, usize) => void
        ptr = ptr >>> 0;
        len = len >>> 0;
        resultPtr = resultPtr >>> 0;
        __module0.fs-read(ptr, len, resultPtr);
      },
    }),
  };
  const { exports } = await WebAssembly.instantiate(module, adaptedImports);
  const memory = exports.memory || imports.env.memory;
  const adaptedExports = Object.setPrototypeOf({
    fsSmoke(oidHi, oidLo) {
      // example/assembly/index/fsSmoke(u64, u64) => void
      oidHi = oidHi || 0n;
      oidLo = oidLo || 0n;
      exports.fsSmoke(oidHi, oidLo);
    },
  }, exports);
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
  return adaptedExports;
}
export const {
  memory,
  smoke,
  fsSmoke,
} = await (async url => instantiate(
  await (async () => {
    const isNodeOrBun = typeof process != "undefined" && process.versions != null && (process.versions.node != null || process.versions.bun != null);
    if (isNodeOrBun) { return globalThis.WebAssembly.compile(await (await import("node:fs/promises")).readFile(url)); }
    else { return await globalThis.WebAssembly.compileStreaming(globalThis.fetch(url)); }
  })(), {
    "mududb:api/system": __maybeDefault(__import0),
  }
))(new URL("example.wasm", import.meta.url));
function __maybeDefault(module) {
  return typeof module.default === "object" && Object.keys(module).length == 1
    ? module.default
    : module;
}
