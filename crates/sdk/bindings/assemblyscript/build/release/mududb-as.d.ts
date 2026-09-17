/** Exported memory */
export declare const memory: WebAssembly.Memory;
/** assembly/fs/FS_O_RDONLY */
export declare const FS_O_RDONLY: {
  /** @type `u32` */
  get value(): number
};
/** assembly/fs/FS_O_RDWR */
export declare const FS_O_RDWR: {
  /** @type `u32` */
  get value(): number
};
/** assembly/fs/FS_O_WRONLY */
export declare const FS_O_WRONLY: {
  /** @type `u32` */
  get value(): number
};
/** assembly/fs/FS_SEEK_CUR */
export declare const FS_SEEK_CUR: {
  /** @type `u32` */
  get value(): number
};
/** assembly/fs/FS_SEEK_END */
export declare const FS_SEEK_END: {
  /** @type `u32` */
  get value(): number
};
/** assembly/fs/FS_SEEK_SET */
export declare const FS_SEEK_SET: {
  /** @type `u32` */
  get value(): number
};
/**
 * assembly/fs/fsClose
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @returns `assembly/result/Result<bool>`
 */
export declare function fsClose(sessionHi: bigint, sessionLo: bigint, fd: number): __Internref4;
/**
 * assembly/fs/fsFstat
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @returns `assembly/result/Result<assembly/fs/FsStat>`
 */
export declare function fsFstat(sessionHi: bigint, sessionLo: bigint, fd: number): __Internref14;
/**
 * assembly/fs/fsFsync
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @returns `assembly/result/Result<bool>`
 */
export declare function fsFsync(sessionHi: bigint, sessionLo: bigint, fd: number): __Internref4;
/**
 * assembly/fs/fsLseek
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @param offset `i64`
 * @param whence `u32`
 * @returns `assembly/result/Result<u64>`
 */
export declare function fsLseek(sessionHi: bigint, sessionLo: bigint, fd: number, offset: bigint, whence: number): __Internref20;
/**
 * assembly/fs/fsOpen
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param oidHi `u64`
 * @param oidLo `u64`
 * @param path `~lib/string/String`
 * @param flags `u32`
 * @returns `assembly/result/Result<u32>`
 */
export declare function fsOpen(sessionHi: bigint, sessionLo: bigint, oidHi: bigint, oidLo: bigint, path: string, flags: number): __Internref22;
/**
 * assembly/fs/fsPread
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @param offset `u64`
 * @param len `u32`
 * @returns `assembly/result/Result<~lib/arraybuffer/ArrayBuffer>`
 */
export declare function fsPread(sessionHi: bigint, sessionLo: bigint, fd: number, offset: bigint, len: number): __Internref25;
/**
 * assembly/fs/fsPwrite
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @param offset `u64`
 * @param data `~lib/arraybuffer/ArrayBuffer`
 * @returns `assembly/result/Result<bool>`
 */
export declare function fsPwrite(sessionHi: bigint, sessionLo: bigint, fd: number, offset: bigint, data: ArrayBuffer): __Internref4;
/**
 * assembly/fs/fsRead
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @param len `u32`
 * @returns `assembly/result/Result<~lib/arraybuffer/ArrayBuffer>`
 */
export declare function fsRead(sessionHi: bigint, sessionLo: bigint, fd: number, len: number): __Internref25;
/**
 * assembly/fs/fsReaddir
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param oidHi `u64`
 * @param oidLo `u64`
 * @param path `~lib/string/String`
 * @returns `assembly/result/Result<~lib/array/Array<assembly/fs/FsDirEntry>>`
 */
export declare function fsReaddir(sessionHi: bigint, sessionLo: bigint, oidHi: bigint, oidLo: bigint, path: string): __Internref32;
/**
 * assembly/fs/fsStat
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param oidHi `u64`
 * @param oidLo `u64`
 * @param path `~lib/string/String`
 * @returns `assembly/result/Result<assembly/fs/FsStat>`
 */
export declare function fsStat(sessionHi: bigint, sessionLo: bigint, oidHi: bigint, oidLo: bigint, path: string): __Internref14;
/**
 * assembly/fs/fsWrite
 * @param sessionHi `u64`
 * @param sessionLo `u64`
 * @param fd `u32`
 * @param data `~lib/arraybuffer/ArrayBuffer`
 * @returns `assembly/result/Result<u32>`
 */
export declare function fsWrite(sessionHi: bigint, sessionLo: bigint, fd: number, data: ArrayBuffer): __Internref22;
/**
 * assembly/procedure/decodeProcedureParam
 * @param paramPtr `usize`
 * @param paramLen `usize`
 * @returns `assembly/generated/UniProcedureParam/UniProcedureParam`
 */
export declare function decodeProcedureParam(paramPtr: number, paramLen: number): __Record40<never>;
/**
 * assembly/procedure/encodeProcedureErr
 * @param error `assembly/wit/MuduError`
 * @param procedure `~lib/string/String`
 * @returns `usize`
 */
export declare function encodeProcedureErr(error: __Internref5, procedure: string): number;
/**
 * assembly/procedure/encodeProcedureOk
 * @param values `assembly/sql/ValueList`
 * @returns `usize`
 */
export declare function encodeProcedureOk(values: __Internref72): number;
/**
 * assembly/procedure/encodeProcedureOkUni
 * @param values `~lib/array/Array<assembly/generated/UniDataValue/UniDataValue>`
 * @returns `usize`
 */
export declare function encodeProcedureOkUni(values: Array<__Record41<undefined>>): number;
/**
 * assembly/record/isNullDatum
 * @param value `assembly/generated/UniDataValue/UniDataValue`
 * @returns `bool`
 */
export declare function isNullDatum(value: __Record41<undefined>): boolean;
/**
 * assembly/record/recordFieldValues
 * @param value `assembly/generated/UniDataValue/UniDataValue`
 * @returns `~lib/typedarray/Uint8Array`
 */
export declare function recordFieldValues(value: __Record41<undefined>): Uint8Array;
/**
 * assembly/record/recordFromFieldValues
 * @param fieldMapBytes `~lib/typedarray/Uint8Array`
 * @returns `assembly/generated/UniDataValue/UniDataValue`
 */
export declare function recordFromFieldValues(fieldMapBytes: Uint8Array): __Record41<never>;
/**
 * assembly/result/procedureResultErr
 * @param error `assembly/wit/MuduError`
 * @param procedure `~lib/string/String`
 * @param location `~lib/string/String`
 * @returns `assembly/result/Result<assembly/sql/ValueList>`
 */
export declare function procedureResultErr(error: __Internref5, procedure?: string, location?: string): __Internref78;
/**
 * assembly/result/procedureResultOk
 * @param values `assembly/sql/ValueList`
 * @returns `assembly/result/Result<assembly/sql/ValueList>`
 */
export declare function procedureResultOk(values: __Internref72): __Internref78;
/**
 * assembly/syscall/witBatch
 * @param id `assembly/wit/Oid`
 * @param stmt `assembly/sql/SqlStmt`
 * @param values `assembly/sql/ValueList`
 * @returns `u64`
 */
export declare function witBatch(id: __Internref13, stmt: __Internref79, values: __Internref72): bigint;
/**
 * assembly/syscall/witClose
 * @param id `assembly/wit/Oid`
 */
export declare function witClose(id: __Internref13): void;
/**
 * assembly/syscall/witCommand
 * @param id `assembly/wit/Oid`
 * @param stmt `assembly/sql/SqlStmt`
 * @param values `assembly/sql/ValueList`
 * @returns `u64`
 */
export declare function witCommand(id: __Internref13, stmt: __Internref79, values: __Internref72): bigint;
/**
 * assembly/syscall/witOpen
 * @param uri `~lib/string/String`
 * @returns `assembly/wit/Oid`
 */
export declare function witOpen(uri: string): __Internref13;
/**
 * assembly/syscall/witQuery
 * @param id `assembly/wit/Oid`
 * @param stmt `assembly/sql/SqlStmt`
 * @param values `assembly/sql/ValueList`
 * @returns `assembly/result/ResultSet`
 */
export declare function witQuery(id: __Internref13, stmt: __Internref79, values: __Internref72): __Internref104;
/** assembly/wit/ERROR_DOMAIN_VIOLATION */
export declare const ERROR_DOMAIN_VIOLATION: {
  /** @type `u32` */
  get value(): number
};
/** assembly/wit/ERROR_ENTITY_NOT_FOUND */
export declare const ERROR_ENTITY_NOT_FOUND: {
  /** @type `u32` */
  get value(): number
};
/** assembly/wit/ERROR_INTERNAL */
export declare const ERROR_INTERNAL: {
  /** @type `u32` */
  get value(): number
};
/** assembly/wit/ERROR_INVALID_ARGUMENT */
export declare const ERROR_INVALID_ARGUMENT: {
  /** @type `u32` */
  get value(): number
};
/** assembly/wit/ValueKind */
export declare enum ValueKind {
  /** @type `i32` */
  Null,
  /** @type `i32` */
  Boolean,
  /** @type `i32` */
  Int64,
  /** @type `i32` */
  Float64,
  /** @type `i32` */
  Text,
  /** @type `i32` */
  Binary,
  /** @type `i32` */
  ObjectId,
}
/**
 * assembly/wit/cabi_realloc
 * @param oldPtr `usize`
 * @param oldSize `usize`
 * @param align `usize`
 * @param newSize `usize`
 * @returns `usize`
 */
export declare function cabi_realloc(oldPtr: number, oldSize: number, align: number, newSize: number): number;
/** assembly/result/Result<bool> */
declare class __Internref4 extends Number {
  private __nominal4: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<assembly/fs/FsStat> */
declare class __Internref14 extends Number {
  private __nominal14: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<u64> */
declare class __Internref20 extends Number {
  private __nominal20: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<u32> */
declare class __Internref22 extends Number {
  private __nominal22: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<~lib/arraybuffer/ArrayBuffer> */
declare class __Internref25 extends Number {
  private __nominal25: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<~lib/array/Array<assembly/fs/FsDirEntry>> */
declare class __Internref32 extends Number {
  private __nominal32: symbol;
  private __nominal0: symbol;
}
/** assembly/generated/UniOid/UniOid */
declare interface __Record16<TOmittable> {
  /** @type `u64` */
  h: bigint | TOmittable;
  /** @type `u64` */
  l: bigint | TOmittable;
}
/** assembly/generated/UniDataValue/UniDataValue */
declare interface __Record41<TOmittable> {
  /** @type `i32` */
  kind: number | TOmittable;
}
/** assembly/generated/UniProcedureParam/UniProcedureParam */
declare interface __Record40<TOmittable> {
  /** @type `u64` */
  procedure: bigint | TOmittable;
  /** @type `assembly/generated/UniOid/UniOid` */
  session: __Record16<never>;
  /** @type `~lib/array/Array<assembly/generated/UniDataValue/UniDataValue>` */
  param_list: Array<__Record41<never>>;
}
/** assembly/wit/MuduError */
declare class __Internref5 extends Number {
  private __nominal5: symbol;
  private __nominal0: symbol;
}
/** assembly/sql/ValueList */
declare class __Internref72 extends Number {
  private __nominal72: symbol;
  private __nominal0: symbol;
}
/** assembly/result/Result<assembly/sql/ValueList> */
declare class __Internref78 extends Number {
  private __nominal78: symbol;
  private __nominal0: symbol;
}
/** assembly/wit/Oid */
declare class __Internref13 extends Number {
  private __nominal13: symbol;
  private __nominal0: symbol;
}
/** assembly/sql/SqlStmt */
declare class __Internref79 extends Number {
  private __nominal79: symbol;
  private __nominal0: symbol;
}
/** assembly/result/ResultSet */
declare class __Internref104 extends Number {
  private __nominal104: symbol;
  private __nominal0: symbol;
}
