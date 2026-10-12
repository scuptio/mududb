#nullable enable

using WalletCsWorld.wit.Imports.mududb.api;

namespace WalletCs;

/// <summary>
/// The uni OID record: two u64 halves, MessagePack map `{1: h, 2: l}`.
/// </summary>
internal readonly struct MuduOid(ulong h, ulong l)
{
    public ulong H { get; } = h;

    public ulong L { get; } = l;
}

/// <summary>
/// The uni error record carried by the `[1, UniError]` result arm:
/// MessagePack map `{1: err_code, 2: err_msg, 3: err_src, 4: err_loc, 5:
/// err_details}` (details travel as an int array, the record-context shape
/// of `list<u8>`).
/// </summary>
internal sealed class MuduUniError(uint code, string message)
{
    public uint Code { get; } = code;

    public string Message { get; } = message;
}

/// <summary>
/// Syscall layer for the wallet-cs guest: SyscallPayload v1 (MSSP) framing
/// plus the `uni` MessagePack wire shapes, mirroring the Mudu.Api SDK's
/// `SyscallPayload`/`MuduSysCallApi` semantics with tag numbers verified
/// against the Rust host (`mudu_binding::universal`). See
/// <see cref="Mp"/> for why the SDK codec itself cannot run in-guest.
///
/// Frame layout (all header fields big-endian):
/// `magic "MSSP" | version 1 | flags 0 | message_kind`, then one MessagePack
/// body. Request bodies are integer-keyed maps of the WIT-declared arguments
/// (1-based parameter numbers); record arguments nest as their own
/// integer-keyed maps (1-based field numbers). Result bodies are
/// `[ok_tag, value]` with `0` = ok and `1` = `UniError`. Decoding is lenient:
/// keys of any integer width, unknown keys skipped, missing fields defaulted.
/// </summary>
internal static class MuduSys
{
    private const uint Magic = 0x4D535350;
    private const uint Version = 1;
    private const uint KindQuery = 1;
    private const uint KindCommand = 2;
    private const int HeaderLen = 16;

    // UniDataValue discriminants (`uni_data_value.rs`).
    private const uint DatScalar = 0;

    // UniScalarValue discriminants used by this guest (`uni_scalar_value.rs`).
    private const uint ScalarI32 = 6;
    private const uint ScalarI64 = 9;
    private const uint ScalarF64 = 12;
    private const uint ScalarString = 14;
    private const uint ScalarBlob = 15;
    private const uint ScalarNumeric = 16;
    private const uint ScalarNull = 21;

    /// Runs a SQL query; returns the rows, each cell decoded as `long` or
    /// `string`. Throws <see cref="MuduUniException"/> on a host error.
    public static global::System.Collections.Generic.List<object?[]> Query(
        MuduOid oid, string sql, params object?[] args)
    {
        var frame = EncodeSqlRequest(KindQuery, oid, sql, args);
        var response = ISystemImports.Query(frame);
        var reader = OpenResult(KindQuery, response, out var error);
        if (error is not null)
        {
            throw new MuduUniException(error);
        }

        // UniQueryResult = {1: tuple_desc, 2: result_set}; the wallet never
        // reads the tuple descriptor.
        var reader2 = reader!;
        var rows = new global::System.Collections.Generic.List<object?[]>();
        var queryResultCount = reader2.ReadMapHeader();
        for (uint i = 0; i < queryResultCount; i++)
        {
            switch (ReadRecordKey(reader2))
            {
                case 2:
                    rows = ReadResultSet(reader2);
                    break;
                case null:
                    break;
                default:
                    reader2.Skip();
                    break;
            }
        }

        return rows;
    }

    /// Runs a SQL command; returns affected rows. Throws
    /// <see cref="MuduUniException"/> on a host error.
    public static ulong Command(MuduOid oid, string sql, params object?[] args)
    {
        var frame = EncodeSqlRequest(KindCommand, oid, sql, args);
        var response = ISystemImports.Command(frame);
        var reader = OpenResult(KindCommand, response, out var error);
        if (error is not null)
        {
            throw new MuduUniException(error);
        }

        // UniCommandResult = {1: affected_rows}.
        var reader2 = reader!;
        ulong affected = 0;
        var commandResultCount = reader2.ReadMapHeader();
        for (uint i = 0; i < commandResultCount; i++)
        {
            switch (ReadRecordKey(reader2))
            {
                case 1:
                    affected = reader2.ReadUInt();
                    break;
                case null:
                    break;
                default:
                    reader2.Skip();
                    break;
            }
        }

        return affected;
    }

    // ---- procedure byte-pipe codec (`handle_procedure`) ----

    /// Decodes the `UniProcedureParam` the host passes to `mp2-*` exports:
    /// `{1: procedure, 2: session, 3: param_list}`. Only scalar i64/string
    /// parameters are supported — the wallet desc declares nothing else.
    public static (ulong Procedure, MuduOid Session, object?[] Params) DecodeProcedureParam(byte[] bytes)
    {
        var reader = new Mp.Reader(bytes);
        ulong procedure = 0;
        var session = new MuduOid(0, 0);
        object?[] args = [];
        var paramCount = reader.ReadMapHeader();
        for (uint i = 0; i < paramCount; i++)
        {
            switch (ReadRecordKey(reader))
            {
                case 1:
                    procedure = reader.ReadUInt();
                    break;
                case 2:
                    session = ReadOid(reader);
                    break;
                case 3:
                    args = ReadParamList(reader);
                    break;
                case null:
                    break;
                default:
                    reader.Skip();
                    break;
            }
        }

        return (procedure, session, args);
    }

    /// Encodes the ok arm of `UniResult<UniProcedureResult, UniError>`: the
    /// single-entry map `{0: {1: [return_list]}}`, mirroring the hand-written
    /// serde impl of `mudu_binding::universal::uni_result::UniResult` with
    /// the record-as-map `UniProcedureResult` payload.
    public static byte[] EncodeProcedureOk(long value)
    {
        var writer = new Mp.Writer();
        writer.WriteMapHeader(1);
        writer.WriteUInt(0);
        writer.WriteMapHeader(1); // UniProcedureResult
        writer.WriteUInt(1);
        writer.WriteArrayHeader(1); // return_list
        WriteDatumI64(writer, value);
        return writer.ToArray();
    }

    /// Encodes the err arm: `{1: UniError}`.
    public static byte[] EncodeProcedureError(uint code, string message)
    {
        var writer = new Mp.Writer();
        writer.WriteMapHeader(1);
        writer.WriteUInt(1);
        WriteUniError(writer, code, message);
        return writer.ToArray();
    }

    // ---- internals ----

    private static byte[] EncodeSqlRequest(uint kind, MuduOid oid, string sql, object?[] args)
    {
        var writer = new Mp.Writer();
        writer.WriteMapHeader(1); // {1: argv}
        writer.WriteUInt(1);
        writer.WriteMapHeader(3); // UniQueryArgv / UniCommandArgv
        writer.WriteUInt(1);
        writer.WriteMapHeader(2); // UniOid
        writer.WriteUInt(1);
        writer.WriteUInt(oid.H);
        writer.WriteUInt(2);
        writer.WriteUInt(oid.L);
        writer.WriteUInt(2);
        writer.WriteMapHeader(1); // UniSqlStmt
        writer.WriteUInt(1);
        writer.WriteString(sql);
        writer.WriteUInt(3);
        writer.WriteMapHeader(1); // UniSqlParam
        writer.WriteUInt(1);
        writer.WriteArrayHeader((uint)args.Length);
        foreach (var arg in args)
        {
            switch (arg)
            {
                case null:
                    WriteDatumNull(writer);
                    break;
                case long l:
                    WriteDatumI64(writer, l);
                    break;
                case int i:
                    WriteDatumI32(writer, i);
                    break;
                case double d:
                    WriteDatumF64(writer, d);
                    break;
                case string s:
                    WriteDatumString(writer, s);
                    break;
                case byte[] b:
                    WriteDatumBlob(writer, b);
                    break;
                case decimal m:
                    WriteDatumNumeric(writer, m);
                    break;
                default:
                    throw new global::System.ArgumentException(
                        $"unsupported SQL parameter type: {arg?.GetType()}");
            }
        }

        var body = writer.ToArray();
        var frame = new byte[HeaderLen + body.Length];
        global::System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(frame.AsSpan(0, 4), Magic);
        global::System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(frame.AsSpan(4, 4), Version);
        // flags at [8..12] stay zero.
        global::System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(frame.AsSpan(12, 4), kind);
        body.CopyTo(frame, HeaderLen);
        return frame;
    }

    /// Validates the response frame header and the `[ok_tag, ...]` body.
    /// Returns the body reader positioned at the value on ok; otherwise the
    /// decoded <see cref="MuduUniError"/> and a null reader.
    private static Mp.Reader? OpenResult(uint expectedKind, byte[] frame, out MuduUniError? error)
    {
        if (frame.Length < HeaderLen
            || global::System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(frame.AsSpan(0, 4)) != Magic
            || global::System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(frame.AsSpan(4, 4)) != Version
            || global::System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(frame.AsSpan(8, 4)) != 0
            || global::System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(frame.AsSpan(12, 4)) != expectedKind)
        {
            throw new global::System.IO.InvalidDataException("MSSP: invalid response frame header");
        }

        var reader = new Mp.Reader(frame[HeaderLen..]);
        ExpectArrayLen(reader.ReadArrayHeader(), 2, "result body");
        var tag = reader.ReadUInt();
        if (tag == 0)
        {
            error = null;
            return reader;
        }

        if (tag != 1)
        {
            throw new global::System.IO.InvalidDataException($"MSSP: unknown result tag {tag}");
        }

        error = ReadUniError(reader);
        return null;
    }

    /// Reads one record-map key. Returns the field number, or `null` when
    /// the key is not an integer — in that case both the key and its value
    /// have already been skipped, mirroring the lenient generated decoders.
    private static long? ReadRecordKey(Mp.Reader reader)
    {
        if (!reader.NextIsInteger())
        {
            reader.Skip();
            reader.Skip();
            return null;
        }

        return reader.ReadInt();
    }

    /// UniResultSet = `{1: eof, 2: row_set, 3: cursor}`; only the rows are
    /// read (eof and the cursor int array are skipped).
    private static global::System.Collections.Generic.List<object?[]> ReadResultSet(Mp.Reader reader)
    {
        var rows = new global::System.Collections.Generic.List<object?[]>();
        var resultSetCount = reader.ReadMapHeader();
        for (uint i = 0; i < resultSetCount; i++)
        {
            switch (ReadRecordKey(reader))
            {
                case 2:
                    rows = ReadRows(reader);
                    break;
                case null:
                    break;
                default:
                    reader.Skip();
                    break;
            }
        }

        return rows;
    }

    private static global::System.Collections.Generic.List<object?[]> ReadRows(Mp.Reader reader)
    {
        var rowCount = reader.ReadArrayHeader();
        var rows = new global::System.Collections.Generic.List<object?[]>((int)rowCount);
        for (uint i = 0; i < rowCount; i++)
        {
            // UniTupleRow = {1: fields}.
            var fields = global::System.Array.Empty<object?>();
            var tupleRowCount = reader.ReadMapHeader();
            for (uint k = 0; k < tupleRowCount; k++)
            {
                switch (ReadRecordKey(reader))
                {
                    case 1:
                        fields = ReadFields(reader);
                        break;
                    case null:
                        break;
                    default:
                        reader.Skip();
                        break;
                }
            }

            rows.Add(fields);
        }

        return rows;
    }

    private static object?[] ReadFields(Mp.Reader reader)
    {
        var fieldCount = reader.ReadArrayHeader();
        var fields = new object?[fieldCount];
        for (uint f = 0; f < fieldCount; f++)
        {
            fields[f] = ReadDatum(reader);
        }

        return fields;
    }

    private static MuduOid ReadOid(Mp.Reader reader)
    {
        ulong h = 0;
        ulong l = 0;
        var oidCount = reader.ReadMapHeader();
        for (uint i = 0; i < oidCount; i++)
        {
            switch (ReadRecordKey(reader))
            {
                case 1:
                    h = reader.ReadUInt();
                    break;
                case 2:
                    l = reader.ReadUInt();
                    break;
                case null:
                    break;
                default:
                    reader.Skip();
                    break;
            }
        }

        return new MuduOid(h, l);
    }

    private static object?[] ReadParamList(Mp.Reader reader)
    {
        var paramCount = reader.ReadArrayHeader();
        var args = new object?[paramCount];
        for (uint i = 0; i < paramCount; i++)
        {
            args[i] = ReadDatum(reader);
        }

        return args;
    }

    private static void WriteDatumI64(Mp.Writer writer, long value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::I64
        writer.WriteUInt(ScalarI64);
        writer.WriteInt(value);
    }

    private static void WriteDatumString(Mp.Writer writer, string value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::String
        writer.WriteUInt(ScalarString);
        writer.WriteString(value);
    }

    private static void WriteDatumNull(Mp.Writer writer)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::Null
        writer.WriteUInt(ScalarNull);
        writer.WriteUInt(0); // payload-less case: the 0u8 placeholder (0x00)
    }

    private static void WriteDatumI32(Mp.Writer writer, int value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::I32
        writer.WriteUInt(ScalarI32);
        writer.WriteInt(value);
    }

    private static void WriteDatumF64(Mp.Writer writer, double value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::F64
        writer.WriteUInt(ScalarF64);
        writer.WriteDouble(value);
    }

    private static void WriteDatumBlob(Mp.Writer writer, byte[] value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::Blob
        writer.WriteUInt(ScalarBlob);
        // Record-context `list<u8>` encodes as an array of u8 (the pinned
        // host quirk), not as MessagePack bin.
        writer.WriteArrayHeader((uint)value.Length);
        foreach (var b in value)
        {
            writer.WriteUInt(b);
        }
    }

    private static void WriteDatumNumeric(Mp.Writer writer, decimal value)
    {
        writer.WriteArrayHeader(2); // UniDataValue::Scalar
        writer.WriteUInt(DatScalar);
        writer.WriteArrayHeader(2); // UniScalarValue::Numeric
        writer.WriteUInt(ScalarNumeric);
        writer.WriteString(value.ToString(global::System.Globalization.CultureInfo.InvariantCulture));
    }

    private static object? ReadDatum(Mp.Reader reader)
    {
        ExpectArrayLen(reader.ReadArrayHeader(), 2, "UniDataValue");
        var kind = reader.ReadUInt();
        if (kind != DatScalar)
        {
            throw new global::System.IO.InvalidDataException(
                $"unsupported UniDataValue kind {kind} (only scalar is supported)");
        }

        ExpectArrayLen(reader.ReadArrayHeader(), 2, "UniScalarValue");
        var scalar = reader.ReadUInt();
        switch (scalar)
        {
            case 0: // Bool
                return reader.ReadBool() ? 1L : 0L;
            case 1: // U8
            case 2: // I8
            case 3: // U16
            case 4: // I16
            case 5: // U32
            case 6: // I32
            case 7: // U64
            case ScalarI64:
                return reader.ReadInt();
            case 13: // Char (a single-character string)
            case ScalarString:
            case 16: // Numeric
            case 17: // Date
            case 18: // Time
            case 19: // Timestamp
            case 20: // TimestampTz
                return reader.ReadString();
            case ScalarNull:
                reader.Skip(); // the 0u8 placeholder
                return null;
            default:
                throw new global::System.IO.InvalidDataException(
                    $"unsupported UniScalarValue tag {scalar}");
        }
    }

    private static void WriteUniError(Mp.Writer writer, uint code, string message)
    {
        writer.WriteMapHeader(5); // UniError
        writer.WriteUInt(1);
        writer.WriteUInt(code);
        writer.WriteUInt(2);
        writer.WriteString(message);
        writer.WriteUInt(3);
        writer.WriteString("wallet-cs"); // err_src
        writer.WriteUInt(4);
        writer.WriteString(string.Empty); // err_loc
        writer.WriteUInt(5);
        writer.WriteArrayHeader(0); // err_details (record-context int array)
    }

    private static MuduUniError ReadUniError(Mp.Reader reader)
    {
        uint code = 0;
        var message = string.Empty;
        var errorCount = reader.ReadMapHeader();
        for (uint i = 0; i < errorCount; i++)
        {
            switch (ReadRecordKey(reader))
            {
                case 1:
                    code = (uint)reader.ReadUInt();
                    break;
                case 2:
                    message = reader.ReadString();
                    break;
                case null:
                    break;
                default:
                    reader.Skip(); // err_src / err_loc / err_details, unknown keys
                    break;
            }
        }

        return new MuduUniError(code, message);
    }

    private static void ExpectArrayLen(uint actual, uint expected, string what)
    {
        if (actual != expected)
        {
            throw new global::System.IO.InvalidDataException(
                $"{what}: expected MessagePack array of length {expected}, got {actual}");
        }
    }
}

/// <summary>
/// A host-side uni error surfaced to the procedure; the byte-pipe dispatcher
/// re-encodes it as the procedure result's error arm.
/// </summary>
internal sealed class MuduUniException(MuduUniError error)
    : global::System.Exception(error.Message)
{
    public MuduUniError Error { get; } = error;
}
