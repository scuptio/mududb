#nullable enable

using Mudu.Api.MuduSys;

namespace Mudu.Api.Mock;

/// <summary>
/// In-memory debug emulation of the host fs syscall family (message kinds
/// FsOpen..FsReaddir). Files live in a flat `(oid, entry) -&gt; byte[]`
/// dictionary and file descriptors in a min-free-uint table with a per-fd
/// cursor. The emulation deliberately implements a small POSIX subset:
///
/// - open access modes 0 (read-only), 1 (write-only) and 2 (read/write); any
///   other flag bit (O_CREAT/O_TRUNC/O_APPEND/O_EXCL style bits) or the
///   reserved mode value 3 is rejected with errno 22 (EINVAL);
/// - a read open of a missing entry fails with errno 2 (ENOENT); a write open
///   creates the entry and truncates it (debug emulation of O_CREAT|O_TRUNC);
/// - read/pread clamp at EOF; write/pwrite extend the file and zero-fill any
///   sparse gap; pread/pwrite do not move the fd cursor;
/// - lseek supports whence 0/1/2 (SET/CUR/END); an unknown whence, a negative
///   result or an overflow is errno 22;
/// - fstat/stat report `{ generation = 1, length, state = 1 }`; stat of a
///   missing entry is errno 2;
/// - readdir lists the immediate children of the path within one fs object;
///   readdir on a file path is errno 20 (ENOTDIR);
/// - operations on an unknown fd fail with errno 9 (EBADF); fsync is a no-op.
///
/// All state is static, process-wide debug state: nothing is persisted and no
/// locking is provided. The fd table is per-process, mirroring the host's
/// min-free fd allocation starting at 0.
/// </summary>
internal static class MockFsEmulation
{
    private const uint ErrnoNoEnt = 2; // ENOENT
    private const uint ErrnoBadF = 9; // EBADF
    private const uint ErrnoNotDir = 20; // ENOTDIR
    private const uint ErrnoInval = 22; // EINVAL

    private const uint AccessModeMask = 3;
    private const uint ModeReadOnly = 0;
    private const uint ModeReadWrite = 2;

    private const ulong EmulatedGeneration = 1;
    private const uint EmulatedState = 1; // SEALED

    private sealed class OpenFile
    {
        public required UniOid Oid { get; init; }

        public required string Entry { get; init; }

        public ulong Cursor { get; set; }
    }

    private static readonly global::System.Collections.Generic.Dictionary<(UniOid Oid, string Entry), byte[]> Files = new();
    private static readonly global::System.Collections.Generic.Dictionary<uint, OpenFile> FdTable = new();

    /// <summary>
    /// Handles one fs request body (header already stripped by the router)
    /// and returns the result body: `[0, value]` on success or
    /// `[1, UniError]` with an errno-coded error.
    /// </summary>
    public static byte[] Handle(MessageKind kind, global::System.ReadOnlyMemory<byte> body)
    {
        return kind switch
        {
            MessageKind.FsOpen => FsOpen(SyscallPayload.DecodeRequestBody<UniFsOpenArgv>(body)),
            MessageKind.FsClose => FsClose(SyscallPayload.DecodeRequestBody<uint>(body)),
            MessageKind.FsRead => FsRead(SyscallPayload.DecodeRequestBody<uint, uint>(body)),
            MessageKind.FsWrite => FsWrite(SyscallPayload.DecodeRequestBody<uint, byte[]>(body)),
            MessageKind.FsPread => FsPread(SyscallPayload.DecodeRequestBody<uint, ulong, uint>(body)),
            MessageKind.FsPwrite => FsPwrite(SyscallPayload.DecodeRequestBody<uint, ulong, byte[]>(body)),
            MessageKind.FsLseek => FsLseek(SyscallPayload.DecodeRequestBody<uint, long, uint>(body)),
            MessageKind.FsFstat => FsFstat(SyscallPayload.DecodeRequestBody<uint>(body)),
            MessageKind.FsStat => FsStat(SyscallPayload.DecodeRequestBody<UniOid, string>(body)),
            MessageKind.FsFsync => FsFsync(SyscallPayload.DecodeRequestBody<uint>(body)),
            MessageKind.FsReaddir => FsReaddir(SyscallPayload.DecodeRequestBody<UniOid, string>(body)),
            _ => throw new global::System.NotSupportedException($"mock fs emulation got unexpected kind {(uint)kind}"),
        };
    }

    private static byte[] FsOpen(UniFsOpenArgv argv)
    {
        var mode = argv.Flags & AccessModeMask;
        if ((argv.Flags & ~AccessModeMask) != 0 || mode > ModeReadWrite)
        {
            return Error(ErrnoInval, $"fs-open: unsupported flags 0x{argv.Flags:X}");
        }

        var key = (argv.Oid, argv.Path);
        if (mode == ModeReadOnly)
        {
            if (!Files.ContainsKey(key))
            {
                return Error(ErrnoNoEnt, $"fs-open: no such entry '{argv.Path}'");
            }
        }
        else
        {
            // Debug emulation: a write open creates the entry and truncates it.
            Files[key] = [];
        }

        var fd = AllocFd();
        FdTable[fd] = new OpenFile { Oid = argv.Oid, Entry = argv.Path, Cursor = 0 };
        return SyscallPayload.EncodeResultBody(fd);
    }

    private static byte[] FsClose(uint fd)
    {
        if (!FdTable.Remove(fd))
        {
            return Error(ErrnoBadF, $"fs-close: unknown fd {fd}");
        }

        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] FsRead((uint Fd, uint Len) args)
    {
        if (!FdTable.TryGetValue(args.Fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-read: unknown fd {args.Fd}");
        }

        var data = Files[(open.Oid, open.Entry)];
        var start = (int)global::System.Math.Min(open.Cursor, (ulong)data.Length);
        var count = (int)global::System.Math.Min(args.Len, (uint)(data.Length - start));
        var result = new byte[count];
        global::System.Array.Copy(data, start, result, 0, count);
        open.Cursor += (uint)count;
        return SyscallPayload.EncodeResultBody(result);
    }

    private static byte[] FsWrite((uint Fd, byte[] Data) args)
    {
        if (!FdTable.TryGetValue(args.Fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-write: unknown fd {args.Fd}");
        }

        var key = (open.Oid, open.Entry);
        Files[key] = WriteAt(Files[key], open.Cursor, args.Data);
        open.Cursor += (uint)args.Data.Length;
        return SyscallPayload.EncodeResultBody((uint)args.Data.Length);
    }

    private static byte[] FsPread((uint Fd, ulong Offset, uint Len) args)
    {
        if (!FdTable.TryGetValue(args.Fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-pread: unknown fd {args.Fd}");
        }

        var data = Files[(open.Oid, open.Entry)];
        var start = (int)global::System.Math.Min(args.Offset, (ulong)data.Length);
        var count = (int)global::System.Math.Min(args.Len, (uint)(data.Length - start));
        var result = new byte[count];
        global::System.Array.Copy(data, start, result, 0, count);
        return SyscallPayload.EncodeResultBody(result);
    }

    private static byte[] FsPwrite((uint Fd, ulong Offset, byte[] Data) args)
    {
        if (!FdTable.TryGetValue(args.Fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-pwrite: unknown fd {args.Fd}");
        }

        var key = (open.Oid, open.Entry);
        Files[key] = WriteAt(Files[key], args.Offset, args.Data);
        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] FsLseek((uint Fd, long Offset, uint Whence) args)
    {
        if (!FdTable.TryGetValue(args.Fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-lseek: unknown fd {args.Fd}");
        }

        var data = Files[(open.Oid, open.Entry)];
        long basePosition;
        switch (args.Whence)
        {
            case 0: // SEEK_SET
                basePosition = 0;
                break;
            case 1: // SEEK_CUR
                basePosition = (long)open.Cursor;
                break;
            case 2: // SEEK_END
                basePosition = data.Length;
                break;
            default:
                return Error(ErrnoInval, $"fs-lseek: unknown whence {args.Whence}");
        }

        long position;
        try
        {
            position = checked(basePosition + args.Offset);
        }
        catch (global::System.OverflowException)
        {
            return Error(ErrnoInval, "fs-lseek: position overflow");
        }

        if (position < 0)
        {
            return Error(ErrnoInval, $"fs-lseek: negative position {position}");
        }

        open.Cursor = (ulong)position;
        return SyscallPayload.EncodeResultBody((ulong)position);
    }

    private static byte[] FsFstat(uint fd)
    {
        if (!FdTable.TryGetValue(fd, out var open))
        {
            return Error(ErrnoBadF, $"fs-fstat: unknown fd {fd}");
        }

        return SyscallPayload.EncodeResultBody(Stat(open.Oid, open.Entry));
    }

    private static byte[] FsStat((UniOid Oid, string Path) args)
    {
        if (!Files.ContainsKey((args.Oid, args.Path)))
        {
            return Error(ErrnoNoEnt, $"fs-stat: no such entry '{args.Path}'");
        }

        return SyscallPayload.EncodeResultBody(Stat(args.Oid, args.Path));
    }

    private static byte[] FsFsync(uint fd)
    {
        if (!FdTable.ContainsKey(fd))
        {
            return Error(ErrnoBadF, $"fs-fsync: unknown fd {fd}");
        }

        // No-op: the emulation holds every byte in memory.
        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] FsReaddir((UniOid Oid, string Path) args)
    {
        if (Files.ContainsKey((args.Oid, args.Path)))
        {
            return Error(ErrnoNotDir, $"fs-readdir: '{args.Path}' is a file");
        }

        var prefix = args.Path.Length == 0
            ? string.Empty
            : args.Path.TrimEnd('/') + "/";
        var entries = new global::System.Collections.Generic.SortedDictionary<string, UniFsDirent>(
            global::System.StringComparer.Ordinal);
        foreach (var ((oid, entry), content) in Files)
        {
            if (!oid.Equals(args.Oid) || !entry.StartsWith(prefix, global::System.StringComparison.Ordinal))
            {
                continue;
            }

            var rest = entry[prefix.Length..];
            if (rest.Length == 0)
            {
                continue;
            }

            var slash = rest.IndexOf('/');
            if (slash >= 0)
            {
                var dirName = rest[..slash];
                if (!entries.ContainsKey(dirName))
                {
                    entries.Add(dirName, new UniFsDirent { Name = dirName, IsDir = true, Length = 0 });
                }
            }
            else
            {
                entries[rest] = new UniFsDirent { Name = rest, IsDir = false, Length = (ulong)content.Length };
            }
        }

        var result = new UniFsDirent[entries.Count];
        entries.Values.CopyTo(result, 0);
        return SyscallPayload.EncodeResultBody(result);
    }

    private static UniFsStat Stat(UniOid oid, string entry)
    {
        return new UniFsStat
        {
            Oid = oid,
            Generation = EmulatedGeneration,
            Entry = entry,
            Length = (ulong)Files[(oid, entry)].Length,
            State = EmulatedState,
        };
    }

    /// Allocates the smallest free fd, mirroring the host's allocation.
    private static uint AllocFd()
    {
        var fd = 0u;
        while (FdTable.ContainsKey(fd))
        {
            fd++;
        }

        return fd;
    }

    /// Writes `data` at `offset`, growing the content and zero-filling any
    /// sparse gap; returns the (possibly reallocated) content.
    private static byte[] WriteAt(byte[] content, ulong offset, byte[] data)
    {
        var end = offset + (ulong)data.Length;
        if (end <= (ulong)content.Length)
        {
            global::System.Array.Copy(data, 0, content, (int)offset, data.Length);
            return content;
        }

        var grown = new byte[end];
        global::System.Array.Copy(content, 0, grown, 0, content.Length);
        global::System.Array.Copy(data, 0, grown, (int)offset, data.Length);
        return grown;
    }

    private static byte[] Error(uint errno, string message)
    {
        return SyscallPayload.EncodeResultErrorBody(new UniError
        {
            ErrCode = errno,
            ErrMsg = message,
            ErrSrc = nameof(MockFsEmulation),
            ErrLoc = string.Empty,
            ErrDetails = [],
        });
    }
}
