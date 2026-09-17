#nullable enable

using mududb.codec;

namespace mududb.mock;

/// <summary>
/// In-memory debug emulation of the host relation syscall family (message
/// kinds RelationGet..RelationInsert), mirroring the semantics of the
/// `relation_*_in_session` kernel handlers at a minimal level:
///
/// - the session field of every frame must name a live emulated session
///   (see <see cref="MockKvEmulation"/>); an unknown session fails with
///   `EntityNotFound` (50009);
/// - tables are per-session maps of primary-key encoding to attribute maps
///   (`attr -&gt; datum`); `relation-insert` auto-creates the table on first
///   use and stores key and value attributes in one row, while get/update on
///   a table that was never inserted into fail with `EntityNotFound`
///   (50009), mirroring the host's `no such relation` lookup;
/// - `relation-insert` on an existing primary key fails with
///   `EntityAlreadyExists` (50012), mirroring the host's duplicate-key rule;
/// - `relation-get` projects the requested attributes in `select` order; a
///   missing row yields an ok-nil result and an attribute the row does not
///   carry projects as a `null` element (SQL NULL);
/// - `relation-update` applies plain value assignments first, then delta
///   assignments, and reports 1 affected row, or 0 when the key does not
///   match any row. Delta operands and targets are interpreted as big-endian
///   i64 datums (8 bytes); a delta targeting an attribute the row does not
///   carry starts from 0. The deferred ops (2/3/4) are applied immediately —
///   the mock has no transaction/commit-apply phase. Op 4
///   (`SubWrapDeferred`) decodes its datum as three big-endian i64s
///   `[q, floor, wrap]` and computes `((cur - floor - q) mod wrap) + floor`.
///   An unknown op fails with `Decode` (50001), mirroring `wire_to_deltas`;
///   malformed numeric datums fail with `InvalidArgument` (50029).
///
/// All state is static, process-wide debug state: nothing is persisted and no
/// locking is provided.
/// </summary>
internal static class MockRelationEmulation
{
    private const uint ErrDecode = 50001;
    private const uint ErrEntityNotFound = 50009;
    private const uint ErrEntityAlreadyExists = 50012;
    private const uint ErrInvalidArgument = 50029;

    private sealed class MockTable
    {
        // row identity: hex of the canonical key-pair encoding
        public global::System.Collections.Generic.Dictionary<string, global::System.Collections.Generic.SortedDictionary<ulong, byte[]>> Rows { get; } = new();
    }

    private static readonly global::System.Collections.Generic.Dictionary<(UniOid SessionId, string Table), MockTable> Tables = new();

    /// <summary>
    /// Handles one relation request body (header already stripped by the
    /// router) and returns the result body: `[0, value]` on success or
    /// `[1, UniError]` with a host error code.
    /// </summary>
    public static byte[] Handle(MessageKind kind, global::System.ReadOnlyMemory<byte> body)
    {
        return kind switch
        {
            MessageKind.RelationGet => RelationGet(
                SyscallPayload.DecodeRequestBody<UniOid, string, (ulong Attr, byte[] Datum)[], ulong[]>(body)),
            MessageKind.RelationUpdate => RelationUpdate(
                SyscallPayload.DecodeRequestBody<UniOid, string, (ulong Attr, byte[] Datum)[], (ulong Attr, byte[] Datum)[], (ulong Attr, byte Op, byte[] Datum)[]>(body)),
            MessageKind.RelationInsert => RelationInsert(
                SyscallPayload.DecodeRequestBody<UniOid, string, (ulong Attr, byte[] Datum)[], (ulong Attr, byte[] Datum)[]>(body)),
            _ => throw new global::System.NotSupportedException($"mock relation emulation got unexpected kind {(uint)kind}"),
        };
    }

    private static byte[] RelationGet(
        (UniOid SessionId, string Table, (ulong Attr, byte[] Datum)[] Key, ulong[] Select) args)
    {
        if (!TryGetTable(args.SessionId, args.Table, out var table, out var error))
        {
            return error;
        }

        if (!table.Rows.TryGetValue(RowId(args.Key), out var row))
        {
            return SyscallPayload.EncodeResultBody<byte[]?[]?>(null);
        }

        var projection = new byte[]?[args.Select.Length];
        for (var i = 0; i < args.Select.Length; i++)
        {
            row.TryGetValue(args.Select[i], out projection[i]);
        }

        return SyscallPayload.EncodeResultBody(projection);
    }

    private static byte[] RelationUpdate(
        (UniOid SessionId, string Table, (ulong Attr, byte[] Datum)[] Key, (ulong Attr, byte[] Datum)[] Values, (ulong Attr, byte Op, byte[] Datum)[] Deltas) args)
    {
        if (!TryGetTable(args.SessionId, args.Table, out var table, out var error))
        {
            return error;
        }

        if (!table.Rows.TryGetValue(RowId(args.Key), out var row))
        {
            return SyscallPayload.EncodeResultBody(0UL);
        }

        foreach (var (attr, datum) in args.Values)
        {
            row[attr] = datum;
        }

        foreach (var delta in args.Deltas)
        {
            if (!TryApplyDelta(row, delta, out error))
            {
                return error;
            }
        }

        return SyscallPayload.EncodeResultBody(1UL);
    }

    private static byte[] RelationInsert(
        (UniOid SessionId, string Table, (ulong Attr, byte[] Datum)[] Key, (ulong Attr, byte[] Datum)[] Values) args)
    {
        if (!MockKvEmulation.SessionExists(args.SessionId))
        {
            return UnknownSession(args.SessionId);
        }

        var tableKey = (args.SessionId, args.Table);
        if (!Tables.TryGetValue(tableKey, out var table))
        {
            table = new MockTable();
            Tables[tableKey] = table;
        }

        var rowId = RowId(args.Key);
        if (table.Rows.ContainsKey(rowId))
        {
            return Error(ErrEntityAlreadyExists, $"relation-insert: duplicate primary key in '{args.Table}'");
        }

        var row = new global::System.Collections.Generic.SortedDictionary<ulong, byte[]>();
        foreach (var (attr, datum) in args.Key)
        {
            row[attr] = datum;
        }

        foreach (var (attr, datum) in args.Values)
        {
            row[attr] = datum;
        }

        table.Rows[rowId] = row;
        return SyscallPayload.EncodeUnitResultBody();
    }

    private static bool TryGetTable(
        UniOid sessionId,
        string table,
        out MockTable mockTable,
        out byte[] error)
    {
        if (!MockKvEmulation.SessionExists(sessionId))
        {
            mockTable = null!;
            error = UnknownSession(sessionId);
            return false;
        }

        if (!Tables.TryGetValue((sessionId, table), out mockTable!))
        {
            error = Error(ErrEntityNotFound, $"no such relation: {table}");
            return false;
        }

        error = null!;
        return true;
    }

    private static bool TryApplyDelta(
        global::System.Collections.Generic.SortedDictionary<ulong, byte[]> row,
        (ulong Attr, byte Op, byte[] Datum) delta,
        out byte[] error)
    {
        long current = 0;
        if (row.TryGetValue(delta.Attr, out var existing) && !TryReadI64(existing, out current))
        {
            error = Error(ErrInvalidArgument, $"relation-update: attr {delta.Attr} datum is not an 8-byte i64");
            return false;
        }

        long updated;
        switch (delta.Op)
        {
            case UniRelationDeltaOp.Add:
            case UniRelationDeltaOp.AddDeferred:
                if (!TryReadI64(delta.Datum, out var addend))
                {
                    error = Error(ErrInvalidArgument, $"relation-update: attr {delta.Attr} delta datum is not an 8-byte i64");
                    return false;
                }

                updated = current + addend;
                break;
            case UniRelationDeltaOp.Sub:
            case UniRelationDeltaOp.SubDeferred:
                if (!TryReadI64(delta.Datum, out var subtrahend))
                {
                    error = Error(ErrInvalidArgument, $"relation-update: attr {delta.Attr} delta datum is not an 8-byte i64");
                    return false;
                }

                updated = current - subtrahend;
                break;
            case UniRelationDeltaOp.SubWrapDeferred:
                if (delta.Datum.Length != 24)
                {
                    error = Error(ErrInvalidArgument, $"relation-update: attr {delta.Attr} sub-wrap datum is not 24 bytes");
                    return false;
                }

                var quantity = global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(delta.Datum.AsSpan(0, 8));
                var floor = global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(delta.Datum.AsSpan(8, 8));
                var wrap = global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(delta.Datum.AsSpan(16, 8));
                if (wrap <= 0)
                {
                    error = Error(ErrInvalidArgument, $"relation-update: attr {delta.Attr} sub-wrap wrap must be positive");
                    return false;
                }

                var residue = (current - floor - quantity) % wrap;
                if (residue < 0)
                {
                    residue += wrap;
                }

                updated = residue + floor;
                break;
            default:
                error = Error(ErrDecode, $"unknown relation delta op {delta.Op}");
                return false;
        }

        var datum = new byte[8];
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(datum, updated);
        row[delta.Attr] = datum;
        error = null!;
        return true;
    }

    private static bool TryReadI64(byte[] datum, out long value)
    {
        if (datum.Length != 8)
        {
            value = 0;
            return false;
        }

        value = global::System.Buffers.Binary.BinaryPrimitives.ReadInt64BigEndian(datum);
        return true;
    }

    /// <summary>
    /// Canonical row identity: each key pair as 8-byte big-endian attr,
    /// 4-byte big-endian datum length, then the datum bytes; rendered as hex
    /// for dictionary use.
    /// </summary>
    private static string RowId((ulong Attr, byte[] Datum)[] key)
    {
        var buffer = new global::System.IO.MemoryStream();
        foreach (var (attr, datum) in key)
        {
            SpanByte(buffer, attr, datum);
        }

        return global::System.Convert.ToHexString(buffer.ToArray());
    }

    private static void SpanByte(global::System.IO.MemoryStream buffer, ulong attr, byte[] datum)
    {
        global::System.Span<byte> header = stackalloc byte[12];
        global::System.Buffers.Binary.BinaryPrimitives.WriteUInt64BigEndian(header.Slice(0, 8), attr);
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt32BigEndian(header.Slice(8, 4), datum.Length);
        buffer.Write(header);
        buffer.Write(datum);
    }

    private static byte[] UnknownSession(UniOid sessionId)
    {
        return Error(ErrEntityNotFound, $"no such session id: {sessionId.H}:{sessionId.L}");
    }

    private static byte[] Error(uint errCode, string message)
    {
        return SyscallPayload.EncodeResultErrorBody(new UniError
        {
            ErrCode = errCode,
            ErrMsg = message,
            ErrSrc = nameof(MockRelationEmulation),
            ErrLoc = string.Empty,
            ErrDetails = [],
        });
    }
}
