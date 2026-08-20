# MuduDB AssemblyScript Binding

This package is the AssemblyScript guest wrapper for
`mududb:component-shim/guest-api`.

The Rust side is `bindings/rs-shim`, which exports
`mududb:component-shim/shim-api` and internally uses the Rust `mududb` facade
crate. The intended wasm target is P2/component-model composition:

```text
AssemblyScript guest component
  imports mududb:component-shim/types
  imports mududb:component-shim/system

Rust rs-shim component
  exports mududb:component-shim/types
  exports mududb:component-shim/system

component compose
  -> final wasm component
```

AssemblyScript does not implement MuduDB encoding, decoding, type layout, SQL
serialization, or database logic.

## Layout

```text
assembly/
  wit.ts       Low-level WIT import declarations.
  database.ts Database facade: open / close / query / command / batch.
  sql.ts       SqlStmt and ValueList wrappers.
  result.ts    ResultSet and Row wrappers.
  fs.ts        FsStat / FsDirEntry types and the fsOpen ... fsReaddir wrappers.
  index.ts     Public exports.

wit/
  api.wit
  async-api.wit
```

## Filesystem (fs) API

`assembly/fs.ts` wraps the 11 synchronous `fs-*` functions of
`mududb:component-shim/system`:

- `fsOpen(sessionHi, sessionLo, oidHi, oidLo, path, flags): Result<u32>`
- `fsClose(sessionHi, sessionLo, fd): Result<bool>`
- `fsRead(sessionHi, sessionLo, fd, len): Result<ArrayBuffer>`
- `fsWrite(sessionHi, sessionLo, fd, data): Result<u32>`
- `fsPread(sessionHi, sessionLo, fd, offset, len): Result<ArrayBuffer>`
- `fsPwrite(sessionHi, sessionLo, fd, offset, data): Result<bool>`
- `fsLseek(sessionHi, sessionLo, fd, offset, whence): Result<u64>`
- `fsFstat(sessionHi, sessionLo, fd): Result<FsStat>`
- `fsStat(sessionHi, sessionLo, oidHi, oidLo, path): Result<FsStat>`
- `fsFsync(sessionHi, sessionLo, fd): Result<bool>`
- `fsReaddir(sessionHi, sessionLo, oidHi, oidLo, path): Result<FsDirEntry[]>`

The session id comes from `Database.open(...).id`. `flags` are the libc
`O_*` access-mode bits (`FS_O_RDONLY` / `FS_O_WRONLY` / `FS_O_RDWR`) and
`whence` is `FS_SEEK_SET` / `FS_SEEK_CUR` / `FS_SEEK_END`.

```ts
const db = Database.open("");
const s = db.id;
const fd = fsOpen(s.hi, s.lo, oidHi, oidLo, "hello.txt", FS_O_WRONLY).unwrap();
fsWrite(s.hi, s.lo, fd, String.UTF8.encode("hello fs", false)).unwrap();
fsClose(s.hi, s.lo, fd).unwrap();
```

The smoke example shows a full write->read roundtrip (`fsSmoke` in
`example/assembly/index.ts`). Only the synchronous fs surface is bound;
async fs awaits AssemblyScript component-model async ABI support.

## Build Core Wasm

```sh
npm install
npm run build
```

Compile the smoke example:

```sh
npx asc example/assembly/index.ts --outFile build/release/example.wasm --optimize
```

The generated core wasm still needs a component adapter that uses
`wit/api.wit` as `guest-api`, then it can be composed with the Rust `rs-shim`
component.
