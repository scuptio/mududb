#nullable enable

namespace Mudu.Api;

/// <summary>
/// Facade for the fs syscall family, mirroring
/// `sys_interface::sync_api::mudu_fs_*`. Every method returns an
/// <see cref="FsResponse{T}"/> (or <see cref="FsResponse"/> for unit calls);
/// failures surface as errno-coded <see cref="UniError"/> values and
/// <c>RequireOk()</c> maps the errno to a BCL exception:
/// ENOENT (2) -&gt; <see cref="global::System.IO.FileNotFoundException"/>,
/// EACCES (13) -&gt; <see cref="global::System.UnauthorizedAccessException"/>,
/// EINVAL (22) -&gt; <see cref="global::System.ArgumentException"/>,
/// EBADF (9) / ENOTDIR (20) / EISDIR (21) and everything else -&gt;
/// <see cref="global::System.IO.IOException"/>.
/// </summary>
public static class MuduFileSystem
{
    /// Open access mode: read-only.
    public const uint OpenReadOnly = 0;

    /// Open access mode: write-only.
    public const uint OpenWriteOnly = 1;

    /// Open access mode: read/write.
    public const uint OpenReadWrite = 2;

    // POSIX errno values surfaced by the fs syscall family.
    private const uint ErrnoNoEnt = 2; // ENOENT
    private const uint ErrnoBadF = 9; // EBADF
    private const uint ErrnoAcces = 13; // EACCES
    private const uint ErrnoNotDir = 20; // ENOTDIR
    private const uint ErrnoIsDir = 21; // EISDIR
    private const uint ErrnoInval = 22; // EINVAL

    /// Open the fs object `oid` (or an entry of it) and return a file descriptor.
    public static FsResponse<uint> FsOpen(UniOid sessionId, UniOid oid, string path, uint flags)
    {
        return FromResult(MuduSysCallApi.SysFsOpen(sessionId, oid, path, flags));
    }

    /// Close an open fs file descriptor.
    public static FsResponse FsClose(UniOid sessionId, uint fd)
    {
        return new FsResponse(MuduSysCallApi.SysFsClose(sessionId, fd));
    }

    /// Read up to `len` bytes at the fd cursor, advancing the cursor.
    public static FsResponse<byte[]> FsRead(UniOid sessionId, uint fd, uint len)
    {
        return FromResult(MuduSysCallApi.SysFsRead(sessionId, fd, len));
    }

    /// Write `data` at the fd cursor, advancing the cursor; returns bytes written.
    public static FsResponse<uint> FsWrite(UniOid sessionId, uint fd, byte[] data)
    {
        return FromResult(MuduSysCallApi.SysFsWrite(sessionId, fd, data));
    }

    /// Read up to `len` bytes at `offset` without moving the fd cursor.
    public static FsResponse<byte[]> FsPread(UniOid sessionId, uint fd, ulong offset, uint len)
    {
        return FromResult(MuduSysCallApi.SysFsPread(sessionId, fd, offset, len));
    }

    /// Write `data` at `offset` without moving the fd cursor.
    public static FsResponse FsPwrite(UniOid sessionId, uint fd, ulong offset, byte[] data)
    {
        return new FsResponse(MuduSysCallApi.SysFsPwrite(sessionId, fd, offset, data));
    }

    /// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new cursor.
    public static FsResponse<ulong> FsLseek(UniOid sessionId, uint fd, long offset, uint whence)
    {
        return FromResult(MuduSysCallApi.SysFsLseek(sessionId, fd, offset, whence));
    }

    /// Stat an open fs file descriptor.
    public static FsResponse<UniFsStat> FsFstat(UniOid sessionId, uint fd)
    {
        return FromResult(MuduSysCallApi.SysFsFstat(sessionId, fd));
    }

    /// Stat the fs object `oid` (or an entry of it) without opening an fd.
    public static FsResponse<UniFsStat> FsStat(UniOid sessionId, UniOid oid, string path)
    {
        return FromResult(MuduSysCallApi.SysFsStat(sessionId, oid, path));
    }

    /// Flush a write fd's content to durable storage.
    public static FsResponse FsFsync(UniOid sessionId, uint fd)
    {
        return new FsResponse(MuduSysCallApi.SysFsFsync(sessionId, fd));
    }

    /// List the entries of an fs object directory.
    public static FsResponse<UniFsDirent[]> FsReaddir(UniOid sessionId, UniOid oid, string path)
    {
        return FromResult(MuduSysCallApi.SysFsReaddir(sessionId, oid, path));
    }

    internal static global::System.Exception ToException(UniError error)
    {
        var errno = error.ErrCode;
        var message = $"errno {errno}: {error.ErrMsg}";
        return errno switch
        {
            ErrnoNoEnt => new global::System.IO.FileNotFoundException(message),
            ErrnoBadF => new global::System.IO.IOException(message),
            ErrnoAcces => new global::System.UnauthorizedAccessException(message),
            ErrnoNotDir => new global::System.IO.IOException(message),
            ErrnoIsDir => new global::System.IO.IOException(message),
            ErrnoInval => new global::System.ArgumentException(message),
            _ => new global::System.IO.IOException(message),
        };
    }

    private static FsResponse<T> FromResult<T>(MuduSys.SyscallResult<T> result)
    {
        return new FsResponse<T>(result.Value, result.Error);
    }
}

/// Response of a value-returning fs syscall, in the spirit of
/// <see cref="CommandResponse"/>/<see cref="QueryResponse"/>.
public readonly struct FsResponse<T>
{
    private readonly T? result;

    internal FsResponse(T? result, UniError? error)
    {
        this.result = result;
        Error = error;
    }

    public bool IsOk => Error is null;

    public bool IsErr => Error is not null;

    public T? Result => result;

    public UniError? Error { get; }

    /// The errno carried by the error, if the call failed.
    public uint? Errno => Error?.ErrCode;

    public T RequireOk()
    {
        if (IsOk)
        {
            return result!;
        }

        throw MuduFileSystem.ToException(Error.GetValueOrDefault());
    }
}

/// Response of a unit fs syscall (`fs-close`, `fs-pwrite`, `fs-fsync`).
public readonly struct FsResponse
{
    internal FsResponse(UniError? error)
    {
        Error = error;
    }

    public bool IsOk => Error is null;

    public bool IsErr => Error is not null;

    public UniError? Error { get; }

    /// The errno carried by the error, if the call failed.
    public uint? Errno => Error?.ErrCode;

    public void RequireOk()
    {
        if (IsErr)
        {
            throw MuduFileSystem.ToException(Error.GetValueOrDefault());
        }
    }
}
