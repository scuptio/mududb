#nullable enable

using MessagePack;
using mududb.codec;

namespace mududb.sys;

public static class MuduSysCallApi
{
    // Host application-level error code mapped to EINVAL at the guest wrapper
    // layer, mirroring `sys_interface::fs::map_fs_errno`. The MSSP frames
    // themselves still carry the host's original code.
    private const uint HostInvalidArgument = 50029;
    private const uint ErrnoInvalidInput = 22; // EINVAL

    /// <summary>
    /// Routes syscalls to the in-process debug mock (SQLite + in-memory fs)
    /// instead of the wasm host imports. The default is enabled only when the
    /// library itself is compiled with `MUDU_MOCK_SQLITE`; native consumers
    /// such as the demo set it explicitly at startup.
    /// </summary>
    public static bool UseMockBackend { get; set; }
#if MUDU_MOCK_SQLITE
        = true;
#endif

    // ---- raw byte-pipe dispatchers ----
    //
    // Every entry point below (except `FetchRaw`) transports one complete
    // SyscallPayload v1 (MSSP) frame in each direction; the frames are opaque
    // to the byte pipe.

    public static byte[] QueryRaw(byte[] queryIn)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.QueryRaw(queryIn)
            : WasmMuduSysCall.QueryRaw(queryIn);
    }

    public static byte[] QueryRaw(global::System.ReadOnlyMemory<byte> queryIn)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.QueryRaw(queryIn)
            : WasmMuduSysCall.QueryRaw(queryIn);
    }

    public static byte[] CommandRaw(byte[] commandIn)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.CommandRaw(commandIn)
            : WasmMuduSysCall.CommandRaw(commandIn);
    }

    public static byte[] CommandRaw(global::System.ReadOnlyMemory<byte> commandIn)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.CommandRaw(commandIn)
            : WasmMuduSysCall.CommandRaw(commandIn);
    }

    // `fetch` has no MSSP route on the host yet; its raw byte path is left
    // unchanged (no 16-byte header is added or stripped here).
    public static byte[] FetchRaw(byte[] queryResult)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FetchRaw(queryResult)
            : WasmMuduSysCall.FetchRaw(queryResult);
    }

    public static byte[] FetchRaw(global::System.ReadOnlyMemory<byte> queryResult)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FetchRaw(queryResult)
            : WasmMuduSysCall.FetchRaw(queryResult);
    }

    public static byte[] FsOpenRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsOpenRaw(frame)
            : WasmMuduSysCall.FsOpenRaw(frame);
    }

    public static byte[] FsCloseRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsCloseRaw(frame)
            : WasmMuduSysCall.FsCloseRaw(frame);
    }

    public static byte[] FsReadRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsReadRaw(frame)
            : WasmMuduSysCall.FsReadRaw(frame);
    }

    public static byte[] FsWriteRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsWriteRaw(frame)
            : WasmMuduSysCall.FsWriteRaw(frame);
    }

    public static byte[] FsPreadRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsPreadRaw(frame)
            : WasmMuduSysCall.FsPreadRaw(frame);
    }

    public static byte[] FsPwriteRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsPwriteRaw(frame)
            : WasmMuduSysCall.FsPwriteRaw(frame);
    }

    public static byte[] FsLseekRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsLseekRaw(frame)
            : WasmMuduSysCall.FsLseekRaw(frame);
    }

    public static byte[] FsFstatRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsFstatRaw(frame)
            : WasmMuduSysCall.FsFstatRaw(frame);
    }

    public static byte[] FsStatRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsStatRaw(frame)
            : WasmMuduSysCall.FsStatRaw(frame);
    }

    public static byte[] FsFsyncRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsFsyncRaw(frame)
            : WasmMuduSysCall.FsFsyncRaw(frame);
    }

    public static byte[] FsReaddirRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.FsReaddirRaw(frame)
            : WasmMuduSysCall.FsReaddirRaw(frame);
    }

    public static byte[] BatchRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.BatchRaw(frame)
            : WasmMuduSysCall.BatchRaw(frame);
    }

    public static byte[] OpenRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.OpenRaw(frame)
            : WasmMuduSysCall.OpenRaw(frame);
    }

    public static byte[] CloseRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.CloseRaw(frame)
            : WasmMuduSysCall.CloseRaw(frame);
    }

    public static byte[] GetRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.GetRaw(frame)
            : WasmMuduSysCall.GetRaw(frame);
    }

    public static byte[] PutRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.PutRaw(frame)
            : WasmMuduSysCall.PutRaw(frame);
    }

    public static byte[] DeleteRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.DeleteRaw(frame)
            : WasmMuduSysCall.DeleteRaw(frame);
    }

    public static byte[] RangeRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.RangeRaw(frame)
            : WasmMuduSysCall.RangeRaw(frame);
    }

    public static byte[] RelationGetRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.RelationGetRaw(frame)
            : WasmMuduSysCall.RelationGetRaw(frame);
    }

    public static byte[] RelationUpdateRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.RelationUpdateRaw(frame)
            : WasmMuduSysCall.RelationUpdateRaw(frame);
    }

    public static byte[] RelationInsertRaw(byte[] frame)
    {
        return UseMockBackend
            ? mududb.mock.MockSqliteMuduSysCall.RelationInsertRaw(frame)
            : WasmMuduSysCall.RelationInsertRaw(frame);
    }

    // ---- SQL syscalls (MSSP frames) ----

    /// <summary>
    /// Encodes a `command` request frame: header plus body `{1: argv}`.
    /// </summary>
    public static byte[] SerializeCommand(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return SyscallPayload.EncodeRequestFrame(MessageKind.Command, argv, options);
    }

    /// <summary>
    /// Encodes a `query` request frame: header plus body `{1: argv}`.
    /// </summary>
    public static byte[] SerializeQuery(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        return SyscallPayload.EncodeRequestFrame(MessageKind.Query, argv, options);
    }

    /// <summary>
    /// Decodes a `command` result frame: header plus body
    /// `[0, UniCommandResult]` / `[1, UniError]`.
    /// </summary>
    public static UniCommandReturn DeserializeCommandResult(byte[] frame, MessagePackSerializerOptions? options = null)
    {
        var result = SyscallPayload.DecodeResultFrame<UniCommandResult>(MessageKind.Command, frame, options);
        return result.IsOk
            ? new UniCommandReturnOk { Inner = result.Value! }
            : new UniCommandReturnErr { Inner = result.Error.GetValueOrDefault() };
    }

    /// <summary>
    /// Decodes a `query` result frame: header plus body
    /// `[0, UniQueryResult]` / `[1, UniError]`.
    /// </summary>
    public static UniQueryReturn DeserializeQueryResult(byte[] frame, MessagePackSerializerOptions? options = null)
    {
        var result = SyscallPayload.DecodeResultFrame<UniQueryResult>(MessageKind.Query, frame, options);
        return result.IsOk
            ? new UniQueryReturnOk { Inner = result.Value! }
            : new UniQueryReturnErr { Inner = result.Error.GetValueOrDefault() };
    }

    public static UniCommandReturn SysCommand(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        var request = SerializeCommand(argv, options);
        var response = CommandRaw(request);
        return DeserializeCommandResult(response, options);
    }

    public static UniQueryReturn SysQuery(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        var request = SerializeQuery(argv, options);
        var response = QueryRaw(request);
        return DeserializeQueryResult(response, options);
    }

    public static ulong SysCommandAffectedRows(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        var result = SysCommand(argv, options);
        return result.Kind() switch
        {
            UniCommandReturnKind.Ok => UniCommandReturnOk.AsOk(result).Inner.AffectedRows,
            UniCommandReturnKind.Err => throw new global::System.InvalidOperationException(UniCommandReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown command result kind"),
        };
    }

    public static UniQueryResult SysQueryOk(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        var result = SysQuery(argv, options);
        return result.Kind() switch
        {
            UniQueryReturnKind.Ok => UniQueryReturnOk.AsOk(result).Inner,
            UniQueryReturnKind.Err => throw new global::System.InvalidOperationException(UniQueryReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown query result kind"),
        };
    }

    // ---- fs syscall family (MSSP frames) ----
    //
    // These mirror `sys_interface::sync_api::mudu_fs_*`: the session id is
    // part of every signature, but the v1 frames carry it only on `fs-open`;
    // all other kinds send the fd-/path-level argument array. Result shapes
    // per kind: open -> u32, read/pread -> bin, write -> u32, lseek -> u64,
    // fstat/stat -> UniFsStat, readdir -> UniFsDirent[], unit kinds -> [0, 0].

    public static SyscallResult<uint> SysFsOpen(UniOid sessionId, UniOid oid, string path, uint flags)
    {
        var argv = new UniFsOpenArgv { Session = sessionId, Oid = oid, Path = path, Flags = flags };
        var response = FsOpenRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsOpen, argv));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<uint>(MessageKind.FsOpen, response));
    }

    public static UniError? SysFsClose(UniOid sessionId, uint fd)
    {
        var response = FsCloseRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsClose, fd));
        return MapFsErrno(SyscallPayload.DecodeUnitResultFrame(MessageKind.FsClose, response));
    }

    public static SyscallResult<byte[]> SysFsRead(UniOid sessionId, uint fd, uint len)
    {
        var response = FsReadRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsRead, fd, len));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<byte[]>(MessageKind.FsRead, response));
    }

    public static SyscallResult<uint> SysFsWrite(UniOid sessionId, uint fd, byte[] data)
    {
        var response = FsWriteRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsWrite, fd, data));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<uint>(MessageKind.FsWrite, response));
    }

    public static SyscallResult<byte[]> SysFsPread(UniOid sessionId, uint fd, ulong offset, uint len)
    {
        var response = FsPreadRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsPread, fd, offset, len));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<byte[]>(MessageKind.FsPread, response));
    }

    public static UniError? SysFsPwrite(UniOid sessionId, uint fd, ulong offset, byte[] data)
    {
        var response = FsPwriteRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsPwrite, fd, offset, data));
        return MapFsErrno(SyscallPayload.DecodeUnitResultFrame(MessageKind.FsPwrite, response));
    }

    public static SyscallResult<ulong> SysFsLseek(UniOid sessionId, uint fd, long offset, uint whence)
    {
        var response = FsLseekRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsLseek, fd, offset, whence));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<ulong>(MessageKind.FsLseek, response));
    }

    public static SyscallResult<UniFsStat> SysFsFstat(UniOid sessionId, uint fd)
    {
        var response = FsFstatRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsFstat, fd));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<UniFsStat>(MessageKind.FsFstat, response));
    }

    public static SyscallResult<UniFsStat> SysFsStat(UniOid sessionId, UniOid oid, string path)
    {
        var response = FsStatRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsStat, oid, path));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<UniFsStat>(MessageKind.FsStat, response));
    }

    public static UniError? SysFsFsync(UniOid sessionId, uint fd)
    {
        var response = FsFsyncRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsFsync, fd));
        return MapFsErrno(SyscallPayload.DecodeUnitResultFrame(MessageKind.FsFsync, response));
    }

    public static SyscallResult<UniFsDirent[]> SysFsReaddir(UniOid sessionId, UniOid oid, string path)
    {
        var response = FsReaddirRaw(SyscallPayload.EncodeRequestFrame(MessageKind.FsReaddir, oid, path));
        return MapFsErrno(SyscallPayload.DecodeResultFrame<UniFsDirent[]>(MessageKind.FsReaddir, response));
    }

    // ---- batch syscall (MSSP frame) ----

    /// <summary>
    /// Encodes a `batch` request frame: header plus body `{1: argv}`.
    /// </summary>
    public static byte[] SerializeBatch(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return SyscallPayload.EncodeRequestFrame(MessageKind.Batch, argv, options);
    }

    /// <summary>
    /// Decodes a `batch` result frame: header plus body
    /// `[0, UniCommandResult]` / `[1, UniError]`.
    /// </summary>
    public static UniCommandReturn DeserializeBatchResult(byte[] frame, MessagePackSerializerOptions? options = null)
    {
        var result = SyscallPayload.DecodeResultFrame<UniCommandResult>(MessageKind.Batch, frame, options);
        return result.IsOk
            ? new UniCommandReturnOk { Inner = result.Value! }
            : new UniCommandReturnErr { Inner = result.Error.GetValueOrDefault() };
    }

    public static UniCommandReturn SysBatch(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        var request = SerializeBatch(argv, options);
        var response = BatchRaw(request);
        return DeserializeBatchResult(response, options);
    }

    // ---- session syscalls (MSSP frames) ----
    //
    // `open` carries the worker OID and returns the new session OID;
    // `close` carries the session OID and returns a unit result.

    /// <summary>
    /// `open-session`: body `[worker_id]`, result `[0, UniOid]` /
    /// `[1, UniError]`.
    /// </summary>
    public static SyscallResult<UniOid> SysOpen(UniOid workerId)
    {
        var response = OpenRaw(SyscallPayload.EncodeRequestFrame(MessageKind.OpenSession, workerId));
        return SyscallPayload.DecodeResultFrame<UniOid>(MessageKind.OpenSession, response);
    }

    /// <summary>
    /// `close-session`: body `[oid]`, result `[0, 0]` / `[1, UniError]`.
    /// Returns the carried error, or `null` on success.
    /// </summary>
    public static UniError? SysClose(UniOid sessionId)
    {
        var response = CloseRaw(SyscallPayload.EncodeRequestFrame(MessageKind.CloseSession, sessionId));
        return SyscallPayload.DecodeUnitResultFrame(MessageKind.CloseSession, response);
    }

    // ---- KV syscall family (MSSP frames) ----
    //
    // Keys and values are MessagePack bin blobs. `range` is start-inclusive /
    // end-exclusive on the host, with an empty end key meaning unbounded; the
    // result pairs arrive sorted by key.

    /// <summary>
    /// `get`: body `[oid, key]`, result `[0, value-or-nil]` /
    /// `[1, UniError]`. A missing key yields an ok result whose
    /// <see cref="SyscallResult{T}.Value"/> is `null`.
    /// </summary>
    public static SyscallResult<byte[]> SysGet(UniOid sessionId, byte[] key)
    {
        var response = GetRaw(SyscallPayload.EncodeRequestFrame(MessageKind.Get, sessionId, key));
        return SyscallPayload.DecodeResultFrame<byte[]>(MessageKind.Get, response);
    }

    /// <summary>
    /// `put`: body `[oid, key, value]`, result `[0, 0]` / `[1, UniError]`.
    /// Returns the carried error, or `null` on success.
    /// </summary>
    public static UniError? SysPut(UniOid sessionId, byte[] key, byte[] value)
    {
        var response = PutRaw(SyscallPayload.EncodeRequestFrame(MessageKind.Put, sessionId, key, value));
        return SyscallPayload.DecodeUnitResultFrame(MessageKind.Put, response);
    }

    /// <summary>
    /// `delete`: body `[oid, key]`, result `[0, 0]` / `[1, UniError]`.
    /// Returns the carried error, or `null` on success.
    /// </summary>
    public static UniError? SysDelete(UniOid sessionId, byte[] key)
    {
        var response = DeleteRaw(SyscallPayload.EncodeRequestFrame(MessageKind.Delete, sessionId, key));
        return SyscallPayload.DecodeUnitResultFrame(MessageKind.Delete, response);
    }

    /// <summary>
    /// `range`: body `[oid, start, end]`, result
    /// `[0, [[key, value], ...]]` / `[1, UniError]`.
    /// </summary>
    public static SyscallResult<(byte[] Key, byte[] Value)[]> SysRange(UniOid sessionId, byte[] start, byte[] end)
    {
        var response = RangeRaw(SyscallPayload.EncodeRequestFrame(MessageKind.Range, sessionId, start, end));
        return SyscallPayload.DecodeResultFrame<(byte[] Key, byte[] Value)[]>(MessageKind.Range, response);
    }

    // ---- relation syscall family (MSSP frames) ----
    //
    // Attributes are column indexes in the original table definition carried
    // as u64; datums are the column's binary encoding carried as MessagePack
    // bin. Key/value pairs encode as `[attr, datum]` 2-arrays, deltas as
    // `[attr, op, datum]` 3-arrays.

    /// <summary>
    /// `relation-get`: body `[oid, table, [[attr, datum], ...], [attr, ...]]`,
    /// result `[0, [[datum-or-nil], ...]-or-nil]` / `[1, UniError]`. A missing
    /// row yields an ok result whose <see cref="SyscallResult{T}.Value"/> is
    /// `null`; a NULL column projects as a `null` element.
    /// </summary>
    public static SyscallResult<byte[]?[]> SysRelationGet(
        UniOid sessionId,
        string table,
        (ulong Attr, byte[] Datum)[] key,
        ulong[] select)
    {
        var response = RelationGetRaw(SyscallPayload.EncodeRequestFrame(MessageKind.RelationGet, sessionId, table, key, select));
        return SyscallPayload.DecodeResultFrame<byte[]?[]>(MessageKind.RelationGet, response);
    }

    /// <summary>
    /// `relation-update`: body `[oid, table, [[attr, datum], ...],
    /// [[attr, datum], ...], [[attr, op, datum], ...]]`, result
    /// `[0, affected]` / `[1, UniError]`.
    /// </summary>
    public static SyscallResult<ulong> SysRelationUpdate(
        UniOid sessionId,
        string table,
        (ulong Attr, byte[] Datum)[] key,
        (ulong Attr, byte[] Datum)[] values,
        UniRelationDelta[] deltas)
    {
        // `uni-syscall.wit` carries deltas as `tuple<u64, u8, list<u8>>`
        // elements; the tuple encoding is the same `[attr, op, datum]`
        // 3-array the historical UniRelationDelta object model produced.
        var deltaTuples = new (ulong Attr, byte Op, byte[] Datum)[deltas.Length];
        for (var i = 0; i < deltas.Length; i++)
        {
            deltaTuples[i] = (deltas[i].Attr, deltas[i].Op, deltas[i].Datum);
        }

        var response = RelationUpdateRaw(SyscallPayload.EncodeRequestFrame(MessageKind.RelationUpdate, sessionId, table, key, values, deltaTuples));
        return SyscallPayload.DecodeResultFrame<ulong>(MessageKind.RelationUpdate, response);
    }

    /// <summary>
    /// `relation-insert`: body `[oid, table, [[attr, datum], ...],
    /// [[attr, datum], ...]]`, result `[0, 0]` / `[1, UniError]`.
    /// Returns the carried error, or `null` on success.
    /// </summary>
    public static UniError? SysRelationInsert(
        UniOid sessionId,
        string table,
        (ulong Attr, byte[] Datum)[] key,
        (ulong Attr, byte[] Datum)[] values)
    {
        var response = RelationInsertRaw(SyscallPayload.EncodeRequestFrame(MessageKind.RelationInsert, sessionId, table, key, values));
        return SyscallPayload.DecodeUnitResultFrame(MessageKind.RelationInsert, response);
    }

    /// <summary>
    /// Maps the host's application-level `InvalidArgument` code (50029) to the
    /// POSIX-facing EINVAL (22), keeping the host's message — the C# mirror of
    /// `sys_interface::fs::map_fs_errno`.
    /// </summary>
    private static UniError MapFsErrno(UniError error)
    {
        if (error.ErrCode != HostInvalidArgument)
        {
            return error;
        }

        return new UniError
        {
            ErrCode = ErrnoInvalidInput,
            ErrMsg = error.ErrMsg,
            ErrSrc = error.ErrSrc,
            ErrLoc = error.ErrLoc,
            ErrDetails = error.ErrDetails,
        };
    }

    private static UniError? MapFsErrno(UniError? error)
    {
        return error is { } value ? MapFsErrno(value) : null;
    }

    private static SyscallResult<T> MapFsErrno<T>(SyscallResult<T> result)
    {
        return result.IsErr
            ? new SyscallResult<T>(MapFsErrno(result.Error.GetValueOrDefault()))
            : result;
    }
}
