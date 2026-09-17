import { UniFsStat } from "./generated/UniFsStat";
import { Result } from "./result";
import { Oid } from "./wit";
import {
  sysFsClose,
  sysFsFstat,
  sysFsFsync,
  sysFsLseek,
  sysFsOpen,
  sysFsPread,
  sysFsPwrite,
  sysFsRead,
  sysFsReaddir,
  sysFsStat,
  sysFsWrite,
} from "./syscall";

export const FS_O_RDONLY: u32 = 0;
export const FS_O_WRONLY: u32 = 1;
export const FS_O_RDWR: u32 = 2;

export const FS_SEEK_SET: u32 = 0;
export const FS_SEEK_CUR: u32 = 1;
export const FS_SEEK_END: u32 = 2;

export class FsStat {
  oid: Oid;
  generation: u64;
  entry: string;
  length: u64;
  state: u32;

  constructor(oid: Oid = new Oid(), generation: u64 = 0, entry: string = "", length: u64 = 0, state: u32 = 0) {
    this.oid = oid;
    this.generation = generation;
    this.entry = entry;
    this.length = length;
    this.state = state;
  }
}

export class FsDirEntry {
  name: string;
  isDir: bool;
  length: u64;

  constructor(name: string = "", isDir: bool = false, length: u64 = 0) {
    this.name = name;
    this.isDir = isDir;
    this.length = length;
  }
}

function mapFsStat(stat: UniFsStat): FsStat {
  return new FsStat(
    new Oid(stat.oid.h, stat.oid.l),
    stat.generation,
    stat.entry,
    stat.length,
    stat.state,
  );
}

// The fd-based fs syscalls and `fs-stat`/`fs-readdir` run under the session
// the procedure invocation is bound to host-side; the session arguments are
// kept in the signatures for API compatibility.

export function fsOpen(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string, flags: u32): Result<u32> {
  return sysFsOpen(new Oid(sessionHi, sessionLo), new Oid(oidHi, oidLo), path, flags);
}

export function fsClose(sessionHi: u64, sessionLo: u64, fd: u32): Result<bool> {
  return sysFsClose(fd);
}

export function fsRead(sessionHi: u64, sessionLo: u64, fd: u32, len: u32): Result<ArrayBuffer> {
  const result = sysFsRead(fd, len);
  if (result.isErr) {
    return Result.error<ArrayBuffer>(result.unwrapErr());
  }
  return Result.ok<ArrayBuffer>(result.unwrap().buffer);
}

export function fsWrite(sessionHi: u64, sessionLo: u64, fd: u32, data: ArrayBuffer): Result<u32> {
  return sysFsWrite(fd, Uint8Array.wrap(data));
}

export function fsPread(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, len: u32): Result<ArrayBuffer> {
  const result = sysFsPread(fd, offset, len);
  if (result.isErr) {
    return Result.error<ArrayBuffer>(result.unwrapErr());
  }
  return Result.ok<ArrayBuffer>(result.unwrap().buffer);
}

export function fsPwrite(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, data: ArrayBuffer): Result<bool> {
  return sysFsPwrite(fd, offset, Uint8Array.wrap(data));
}

export function fsLseek(sessionHi: u64, sessionLo: u64, fd: u32, offset: i64, whence: u32): Result<u64> {
  return sysFsLseek(fd, offset, whence);
}

export function fsFstat(sessionHi: u64, sessionLo: u64, fd: u32): Result<FsStat> {
  const result = sysFsFstat(fd);
  if (result.isErr) {
    return Result.error<FsStat>(result.unwrapErr());
  }
  return Result.ok<FsStat>(mapFsStat(result.unwrap()));
}

export function fsStat(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string): Result<FsStat> {
  const result = sysFsStat(new Oid(oidHi, oidLo), path);
  if (result.isErr) {
    return Result.error<FsStat>(result.unwrapErr());
  }
  return Result.ok<FsStat>(mapFsStat(result.unwrap()));
}

export function fsFsync(sessionHi: u64, sessionLo: u64, fd: u32): Result<bool> {
  return sysFsFsync(fd);
}

export function fsReaddir(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string): Result<FsDirEntry[]> {
  const result = sysFsReaddir(new Oid(oidHi, oidLo), path);
  if (result.isErr) {
    return Result.error<FsDirEntry[]>(result.unwrapErr());
  }
  const entries = result.unwrap();
  const out: FsDirEntry[] = [];
  for (let i = 0; i < entries.length; i++) {
    const entry = entries[i];
    out.push(new FsDirEntry(entry.name, entry.is_dir, entry.length));
  }
  return Result.ok<FsDirEntry[]>(out);
}
