#nullable enable

using MessagePack;
using Mudu.Api.MuduSys;

namespace Mudu.Api;

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
            ? Mock.MockSqliteMuduSysCall.QueryRaw(queryIn)
            : MuduSys.WasmMuduSysCall.QueryRaw(queryIn);
    }

    public static byte[] QueryRaw(global::System.ReadOnlyMemory<byte> queryIn)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.QueryRaw(queryIn)
            : MuduSys.WasmMuduSysCall.QueryRaw(queryIn);
    }

    public static byte[] CommandRaw(byte[] commandIn)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.CommandRaw(commandIn)
            : MuduSys.WasmMuduSysCall.CommandRaw(commandIn);
    }

    public static byte[] CommandRaw(global::System.ReadOnlyMemory<byte> commandIn)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.CommandRaw(commandIn)
            : MuduSys.WasmMuduSysCall.CommandRaw(commandIn);
    }

    // `fetch` has no MSSP route on the host yet; its raw byte path is left
    // unchanged (no 16-byte header is added or stripped here).
    public static byte[] FetchRaw(byte[] queryResult)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FetchRaw(queryResult)
            : MuduSys.WasmMuduSysCall.FetchRaw(queryResult);
    }

    public static byte[] FetchRaw(global::System.ReadOnlyMemory<byte> queryResult)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FetchRaw(queryResult)
            : MuduSys.WasmMuduSysCall.FetchRaw(queryResult);
    }

    public static byte[] FsOpenRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsOpenRaw(frame)
            : MuduSys.WasmMuduSysCall.FsOpenRaw(frame);
    }

    public static byte[] FsCloseRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsCloseRaw(frame)
            : MuduSys.WasmMuduSysCall.FsCloseRaw(frame);
    }

    public static byte[] FsReadRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsReadRaw(frame)
            : MuduSys.WasmMuduSysCall.FsReadRaw(frame);
    }

    public static byte[] FsWriteRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsWriteRaw(frame)
            : MuduSys.WasmMuduSysCall.FsWriteRaw(frame);
    }

    public static byte[] FsPreadRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsPreadRaw(frame)
            : MuduSys.WasmMuduSysCall.FsPreadRaw(frame);
    }

    public static byte[] FsPwriteRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsPwriteRaw(frame)
            : MuduSys.WasmMuduSysCall.FsPwriteRaw(frame);
    }

    public static byte[] FsLseekRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsLseekRaw(frame)
            : MuduSys.WasmMuduSysCall.FsLseekRaw(frame);
    }

    public static byte[] FsFstatRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsFstatRaw(frame)
            : MuduSys.WasmMuduSysCall.FsFstatRaw(frame);
    }

    public static byte[] FsStatRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsStatRaw(frame)
            : MuduSys.WasmMuduSysCall.FsStatRaw(frame);
    }

    public static byte[] FsFsyncRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsFsyncRaw(frame)
            : MuduSys.WasmMuduSysCall.FsFsyncRaw(frame);
    }

    public static byte[] FsReaddirRaw(byte[] frame)
    {
        return UseMockBackend
            ? Mock.MockSqliteMuduSysCall.FsReaddirRaw(frame)
            : MuduSys.WasmMuduSysCall.FsReaddirRaw(frame);
    }

    // ---- SQL syscalls (MSSP frames) ----

    /// <summary>
    /// Encodes a `command` request frame: header plus body `[argv]`.
    /// </summary>
    public static byte[] SerializeCommand(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return SyscallPayload.EncodeRequestFrame(MessageKind.Command, argv, options);
    }

    /// <summary>
    /// Encodes a `query` request frame: header plus body `[argv]`.
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
