import { Result } from "./result";
import {
  ERROR_RESULT_VALUE_OFFSET_4,
  ERROR_RESULT_VALUE_OFFSET_8,
  Oid,
  RESULT_ERROR_SIZE,
  alloc,
  liftError,
  liftString,
  resultIsOk,
  utf8Bytes,
} from "./wit";

export const FS_O_RDONLY: u32 = 0;
export const FS_O_WRONLY: u32 = 1;
export const FS_O_RDWR: u32 = 2;

export const FS_SEEK_SET: u32 = 0;
export const FS_SEEK_CUR: u32 = 1;
export const FS_SEEK_END: u32 = 2;

const FS_STAT_ENTRY_PTR_OFFSET: usize = 24;
const FS_STAT_ENTRY_LEN_OFFSET: usize = 28;
const FS_STAT_LENGTH_OFFSET: usize = 32;
const FS_STAT_STATE_OFFSET: usize = 40;
const FS_STAT_RESULT_SIZE: usize = 56;
const FS_DIRENT_NAME_LEN_OFFSET: usize = 4;
const FS_DIRENT_IS_DIR_OFFSET: usize = 8;
const FS_DIRENT_LENGTH_OFFSET: usize = 16;
const FS_DIRENT_SIZE: usize = 24;

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

@external("mududb:component-shim/system", "fs-open")
declare function rawFsOpen(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, pathPtr: usize, pathLen: usize, flags: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-close")
declare function rawFsClose(sessionHi: u64, sessionLo: u64, fd: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-read")
declare function rawFsRead(sessionHi: u64, sessionLo: u64, fd: u32, len: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-write")
declare function rawFsWrite(sessionHi: u64, sessionLo: u64, fd: u32, dataPtr: usize, dataLen: usize, result: usize): void;
@external("mududb:component-shim/system", "fs-pread")
declare function rawFsPread(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, len: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-pwrite")
declare function rawFsPwrite(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, dataPtr: usize, dataLen: usize, result: usize): void;
@external("mududb:component-shim/system", "fs-lseek")
declare function rawFsLseek(sessionHi: u64, sessionLo: u64, fd: u32, offset: i64, whence: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-fstat")
declare function rawFsFstat(sessionHi: u64, sessionLo: u64, fd: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-stat")
declare function rawFsStat(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, pathPtr: usize, pathLen: usize, result: usize): void;
@external("mududb:component-shim/system", "fs-fsync")
declare function rawFsFsync(sessionHi: u64, sessionLo: u64, fd: u32, result: usize): void;
@external("mududb:component-shim/system", "fs-readdir")
declare function rawFsReaddir(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, pathPtr: usize, pathLen: usize, result: usize): void;

function liftBuffer(ptr: usize, len: usize): ArrayBuffer {
  const out = new ArrayBuffer(<i32>len);
  memory.copy(changetype<usize>(out), ptr, len);
  return out;
}

function liftFsStat(ptr: usize): FsStat {
  return new FsStat(
    new Oid(load<u64>(ptr), load<u64>(ptr + 8)),
    load<u64>(ptr + 16),
    liftString(load<u32>(ptr + FS_STAT_ENTRY_PTR_OFFSET), load<u32>(ptr + FS_STAT_ENTRY_LEN_OFFSET)),
    load<u64>(ptr + FS_STAT_LENGTH_OFFSET),
    load<u32>(ptr + FS_STAT_STATE_OFFSET),
  );
}

function liftFsDirEntries(ptr: usize, len: u32): FsDirEntry[] {
  const entries: FsDirEntry[] = [];
  for (let i: u32 = 0; i < len; i++) {
    const base = ptr + <usize>i * FS_DIRENT_SIZE;
    entries.push(new FsDirEntry(
      liftString(load<u32>(base), load<u32>(base + FS_DIRENT_NAME_LEN_OFFSET)),
      load<u8>(base + FS_DIRENT_IS_DIR_OFFSET) != 0,
      load<u64>(base + FS_DIRENT_LENGTH_OFFSET),
    ));
  }
  return entries;
}

export function fsOpen(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string, flags: u32): Result<u32> {
  const bytes = utf8Bytes(path);
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsOpen(sessionHi, sessionLo, oidHi, oidLo, changetype<usize>(bytes), bytes.byteLength, flags, out);
  return resultIsOk(out)
    ? Result.ok<u32>(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4))
    : Result.error<u32>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsClose(sessionHi: u64, sessionLo: u64, fd: u32): Result<bool> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsClose(sessionHi, sessionLo, fd, out);
  return resultIsOk(out)
    ? Result.ok<bool>(true)
    : Result.error<bool>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsRead(sessionHi: u64, sessionLo: u64, fd: u32, len: u32): Result<ArrayBuffer> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsRead(sessionHi, sessionLo, fd, len, out);
  return resultIsOk(out)
    ? Result.ok<ArrayBuffer>(liftBuffer(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : Result.error<ArrayBuffer>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsWrite(sessionHi: u64, sessionLo: u64, fd: u32, data: ArrayBuffer): Result<u32> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsWrite(sessionHi, sessionLo, fd, changetype<usize>(data), data.byteLength, out);
  return resultIsOk(out)
    ? Result.ok<u32>(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4))
    : Result.error<u32>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsPread(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, len: u32): Result<ArrayBuffer> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsPread(sessionHi, sessionLo, fd, offset, len, out);
  return resultIsOk(out)
    ? Result.ok<ArrayBuffer>(liftBuffer(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : Result.error<ArrayBuffer>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsPwrite(sessionHi: u64, sessionLo: u64, fd: u32, offset: u64, data: ArrayBuffer): Result<bool> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsPwrite(sessionHi, sessionLo, fd, offset, changetype<usize>(data), data.byteLength, out);
  return resultIsOk(out)
    ? Result.ok<bool>(true)
    : Result.error<bool>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsLseek(sessionHi: u64, sessionLo: u64, fd: u32, offset: i64, whence: u32): Result<u64> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsLseek(sessionHi, sessionLo, fd, offset, whence, out);
  return resultIsOk(out)
    ? Result.ok<u64>(load<u64>(out + ERROR_RESULT_VALUE_OFFSET_8))
    : Result.error<u64>(liftError(out + ERROR_RESULT_VALUE_OFFSET_8));
}

export function fsFstat(sessionHi: u64, sessionLo: u64, fd: u32): Result<FsStat> {
  const out = alloc(FS_STAT_RESULT_SIZE);
  rawFsFstat(sessionHi, sessionLo, fd, out);
  return resultIsOk(out)
    ? Result.ok<FsStat>(liftFsStat(out + ERROR_RESULT_VALUE_OFFSET_8))
    : Result.error<FsStat>(liftError(out + ERROR_RESULT_VALUE_OFFSET_8));
}

export function fsStat(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string): Result<FsStat> {
  const bytes = utf8Bytes(path);
  const out = alloc(FS_STAT_RESULT_SIZE);
  rawFsStat(sessionHi, sessionLo, oidHi, oidLo, changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? Result.ok<FsStat>(liftFsStat(out + ERROR_RESULT_VALUE_OFFSET_8))
    : Result.error<FsStat>(liftError(out + ERROR_RESULT_VALUE_OFFSET_8));
}

export function fsFsync(sessionHi: u64, sessionLo: u64, fd: u32): Result<bool> {
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsFsync(sessionHi, sessionLo, fd, out);
  return resultIsOk(out)
    ? Result.ok<bool>(true)
    : Result.error<bool>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}

export function fsReaddir(sessionHi: u64, sessionLo: u64, oidHi: u64, oidLo: u64, path: string): Result<FsDirEntry[]> {
  const bytes = utf8Bytes(path);
  const out = alloc(RESULT_ERROR_SIZE);
  rawFsReaddir(sessionHi, sessionLo, oidHi, oidLo, changetype<usize>(bytes), bytes.byteLength, out);
  return resultIsOk(out)
    ? Result.ok<FsDirEntry[]>(liftFsDirEntries(load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4), load<u32>(out + ERROR_RESULT_VALUE_OFFSET_4 + 4)))
    : Result.error<FsDirEntry[]>(liftError(out + ERROR_RESULT_VALUE_OFFSET_4));
}
