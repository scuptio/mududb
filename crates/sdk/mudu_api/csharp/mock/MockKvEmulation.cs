#nullable enable

using mududb.codec;

namespace mududb.mock;

/// <summary>
/// In-memory debug emulation of the host session and KV syscall families
/// (message kinds Open..Range). Every `open` allocates a synthetic session
/// OID from a process-wide counter and an empty sorted byte-key store; the KV
/// kinds address that store by session OID:
///
/// - get returns the stored value or an ok-nil result for a missing key;
/// - put inserts or replaces; delete of a missing key is a no-op success;
/// - range is start-inclusive / end-exclusive over keys sorted
///   lexicographically (unsigned byte order), with an empty end key meaning
///   unbounded — mirroring `WorkerStorage::kv_range`; the result pairs are
///   emitted in ascending key order;
/// - any KV kind naming an unknown session fails with the host's
///   `EntityNotFound` code (50009), mirroring `get_context`.
///
/// All state is static, process-wide debug state: nothing is persisted and no
/// locking is provided.
/// </summary>
internal static class MockKvEmulation
{
    private const uint ErrEntityNotFound = 50009;

    private static readonly global::System.Collections.Generic.Dictionary<UniOid, global::System.Collections.Generic.SortedDictionary<byte[], byte[]>> Sessions = new();
    private static ulong nextSessionLow;

    /// <summary>
    /// Handles one session/KV request body (header already stripped by the
    /// router) and returns the result body: `[0, value]` on success or
    /// `[1, UniError]` with a host error code.
    /// </summary>
    public static byte[] Handle(MessageKind kind, global::System.ReadOnlyMemory<byte> body)
    {
        return kind switch
        {
            MessageKind.OpenSession => Open(SyscallPayload.DecodeRequestBody<UniOid>(body)),
            MessageKind.CloseSession => Close(SyscallPayload.DecodeRequestBody<UniOid>(body)),
            MessageKind.Get => Get(SyscallPayload.DecodeRequestBody<UniOid, byte[]>(body)),
            MessageKind.Put => Put(SyscallPayload.DecodeRequestBody<UniOid, byte[], byte[]>(body)),
            MessageKind.Delete => Delete(SyscallPayload.DecodeRequestBody<UniOid, byte[]>(body)),
            MessageKind.Range => Range(SyscallPayload.DecodeRequestBody<UniOid, byte[], byte[]>(body)),
            _ => throw new global::System.NotSupportedException($"mock kv emulation got unexpected kind {(uint)kind}"),
        };
    }

    /// <summary>
    /// Whether `oid` names a live emulated session; shared with the relation
    /// emulation, which resolves its frames' session field the same way.
    /// </summary>
    public static bool SessionExists(UniOid oid)
    {
        return Sessions.ContainsKey(oid);
    }

    private static byte[] Open(UniOid workerId)
    {
        var sessionId = new UniOid { H = 0, L = ++nextSessionLow };
        Sessions[sessionId] = new global::System.Collections.Generic.SortedDictionary<byte[], byte[]>(ByteArrayComparer.Instance);
        return SyscallPayload.EncodeResultBody(sessionId);
    }

    private static byte[] Close(UniOid sessionId)
    {
        if (!Sessions.Remove(sessionId))
        {
            return UnknownSession(sessionId);
        }

        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] Get((UniOid SessionId, byte[] Key) args)
    {
        if (!Sessions.TryGetValue(args.SessionId, out var store))
        {
            return UnknownSession(args.SessionId);
        }

        return SyscallPayload.EncodeResultBody(store.TryGetValue(args.Key, out var value) ? value : null);
    }

    private static byte[] Put((UniOid SessionId, byte[] Key, byte[] Value) args)
    {
        if (!Sessions.TryGetValue(args.SessionId, out var store))
        {
            return UnknownSession(args.SessionId);
        }

        store[args.Key] = args.Value;
        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] Delete((UniOid SessionId, byte[] Key) args)
    {
        if (!Sessions.TryGetValue(args.SessionId, out var store))
        {
            return UnknownSession(args.SessionId);
        }

        store.Remove(args.Key);
        return SyscallPayload.EncodeUnitResultBody();
    }

    private static byte[] Range((UniOid SessionId, byte[] Start, byte[] End) args)
    {
        if (!Sessions.TryGetValue(args.SessionId, out var store))
        {
            return UnknownSession(args.SessionId);
        }

        var items = new global::System.Collections.Generic.List<(byte[] Key, byte[] Value)>();
        foreach (var (key, value) in store)
        {
            var afterStart = ByteArrayComparer.Instance.Compare(key, args.Start) >= 0;
            var beforeEnd = args.End.Length == 0 || ByteArrayComparer.Instance.Compare(key, args.End) < 0;
            if (afterStart && beforeEnd)
            {
                items.Add((key, value));
            }
        }

        return SyscallPayload.EncodeResultBody(items.ToArray());
    }

    private static byte[] UnknownSession(UniOid sessionId)
    {
        return SyscallPayload.EncodeResultErrorBody(new UniError
        {
            ErrCode = ErrEntityNotFound,
            ErrMsg = $"no such session id: {sessionId.H}:{sessionId.L}",
            ErrSrc = nameof(MockKvEmulation),
            ErrLoc = string.Empty,
            ErrDetails = [],
        });
    }

    /// <summary>
    /// Unsigned lexicographic byte order, matching the host's `&[u8]`
    /// comparisons (`&lt;` on slices).
    /// </summary>
    internal sealed class ByteArrayComparer : global::System.Collections.Generic.IComparer<byte[]>
    {
        public static readonly ByteArrayComparer Instance = new();

        public int Compare(byte[]? x, byte[]? y)
        {
            if (x is null || y is null)
            {
                return (x is null ? 0 : 1) - (y is null ? 0 : 1);
            }

            var shared = global::System.Math.Min(x.Length, y.Length);
            for (var i = 0; i < shared; i++)
            {
                if (x[i] != y[i])
                {
                    return x[i] < y[i] ? -1 : 1;
                }
            }

            return x.Length.CompareTo(y.Length);
        }
    }
}
