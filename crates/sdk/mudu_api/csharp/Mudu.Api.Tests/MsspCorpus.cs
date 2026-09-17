#nullable enable

using System.Globalization;
using System.Text.Json;
using mududb;
using mududb.codec;
using mududb.types;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Shared driver for the MSSP v1 golden corpora: it decodes one fixture
/// frame and compares it against the semantic expectations of the JSON
/// sidecar produced by the Rust host generator
/// (`generate_golden_v1_fixtures` in the `testing` crate). The sidecar is
/// the single source of truth — no expected value is hardcoded here.
///
/// Sidecar conventions (see the `conventions` object of either sidecar):
/// u64/i64 values are decimal strings, u8/u32 values are plain JSON numbers,
/// byte strings are lowercase hex in `*_hex` fields, a UniOid is
/// `{"h", "l"}` of decimal strings, a unit result is `{"unit": true}`, and a
/// relation row is an array of cells (hex string or null; a null row means
/// no row found).
///
/// For canonical frames (the `syscall_payload_v1_all` corpus) request
/// vectors additionally pin the encoder: re-encoding the decoded arguments
/// must reproduce the committed frame byte for byte, and ok responses go
/// through a decode → re-encode → decode roundtrip that must stay
/// semantically equal. Lenient vectors (the `lenient_decode_v1` corpus) are
/// hand-built non-canonical bytes, so they are decode-and-compare only.
/// </summary>
internal static class MsspCorpus
{
    /// Locates a repository-root-relative fixture by walking up from the
    /// test assembly directory, so tests read the committed files in place
    /// instead of copied duplicates.
    public static string FixturePath(string name)
    {
        const string relative = "crates/db-kernel/testing/fixtures/golden/v1";
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            var candidate = Path.Combine(dir.FullName, relative, name);
            if (File.Exists(candidate))
            {
                return candidate;
            }

            dir = dir.Parent;
        }

        throw new FileNotFoundException(
            $"golden corpus {name} not found above {AppContext.BaseDirectory}; " +
            "run generate_golden_v1_fixtures in the testing crate");
    }

    /// Splits fixture bytes into the individual segments, each prefixed with
    /// its big-endian u32 byte length.
    public static List<byte[]> UnpackSegments(byte[] bytes)
    {
        var segments = new List<byte[]>();
        var offset = 0;
        while (offset < bytes.Length)
        {
            var length = (int)global::System.Buffers.Binary.BinaryPrimitives
                .ReadUInt32BigEndian(bytes.AsSpan(offset, 4));
            offset += 4;
            segments.Add(bytes[offset..(offset + length)]);
            offset += length;
        }

        return segments;
    }

    /// Decodes one frame and asserts it against its sidecar entry. `vector`
    /// carries `message_kind`, `message_kind_name`, `direction` and the
    /// kind-specific `expect` object. When `canonical` is set the frame is
    /// canonical encoder output and the encoder pins described on the class
    /// are applied.
    public static void AssertVector(byte[] frame, JsonElement vector, bool canonical)
    {
        var kind = (MessageKind)vector.GetProperty("message_kind").GetInt32();
        var kindName = vector.GetProperty("message_kind_name").GetString()!;
        var direction = vector.GetProperty("direction").GetString()!;
        var expect = vector.GetProperty("expect");
        Assert.Equal(kind, SyscallPayload.DecodeHeader(frame));

        switch ((kindName, direction))
        {
            case ("query", "request"):
                AssertRequest(frame, expect, canonical, f => UniSyscall.DecodeQueryRequest(f), f => UniSyscall.EncodeQueryRequest(f), AssertQueryArgv);
                break;
            case ("query", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeQueryResult(f), f => UniSyscall.EncodeQueryResult(f), AssertQueryResult);
                break;
            case ("command", "request"):
                AssertRequest(frame, expect, canonical, f => UniSyscall.DecodeCommandRequest(f), f => UniSyscall.EncodeCommandRequest(f), AssertCommandArgv);
                break;
            case ("command", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeCommandResult(f), f => UniSyscall.EncodeCommandResult(f), AssertCommandResult);
                break;
            case ("batch", "request"):
                AssertRequest(frame, expect, canonical, f => UniSyscall.DecodeBatchRequest(f), f => UniSyscall.EncodeBatchRequest(f), AssertCommandArgv);
                break;
            case ("batch", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeBatchResult(f), f => UniSyscall.EncodeBatchResult(f), AssertCommandResult);
                break;
            case ("open-session", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeOpenSessionRequest(f),
                    f => UniSyscall.EncodeOpenSessionRequest(f),
                    (oid, e) => AssertOid(oid, e.GetProperty("worker_oid")));
                break;
            case ("open-session", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeOpenSessionResult(f),
                    f => UniSyscall.EncodeOpenSessionResult(f),
                    (oid, e) => Assert.Equal(new UniOid { H = 0, L = U64(e.GetProperty("session")) }, oid));
                break;
            case ("close-session", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeCloseSessionRequest(f),
                    f => UniSyscall.EncodeCloseSessionRequest(f),
                    (oid, e) => AssertOid(oid, e.GetProperty("oid")));
                break;
            case ("close-session", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeCloseSessionResult(f), f => UniSyscall.EncodeCloseSessionResult(f));
                break;
            case ("get", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeGetRequest(f),
                    a => UniSyscall.EncodeGetRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(Hex(e.GetProperty("key_hex")), a.Item2);
                    });
                break;
            case ("get", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeGetResult(f),
                    f => UniSyscall.EncodeGetResult(f),
                    (value, e) => Assert.Equal(Hex(e.GetProperty("value_hex")), value));
                break;
            case ("get", "response_err"):
                AssertErrorResponse(frame, expect, f => UniSyscall.DecodeGetResult(f));
                break;
            case ("put", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodePutRequest(f),
                    a => UniSyscall.EncodePutRequest(a.Item1, a.Item2, a.Item3),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(Hex(e.GetProperty("key_hex")), a.Item2);
                        Assert.Equal(Hex(e.GetProperty("value_hex")), a.Item3);
                    });
                break;
            case ("put", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodePutResult(f), f => UniSyscall.EncodePutResult(f));
                break;
            case ("delete", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeDeleteRequest(f),
                    a => UniSyscall.EncodeDeleteRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(Hex(e.GetProperty("key_hex")), a.Item2);
                    });
                break;
            case ("delete", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeDeleteResult(f), f => UniSyscall.EncodeDeleteResult(f));
                break;
            case ("range", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeRangeRequest(f),
                    a => UniSyscall.EncodeRangeRequest(a.Item1, a.Item2, a.Item3),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(Hex(e.GetProperty("start_hex")), a.Item2);
                        Assert.Equal(Hex(e.GetProperty("end_hex")), a.Item3);
                    });
                break;
            case ("range", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeRangeResult(f), f => UniSyscall.EncodeRangeResult(f), AssertRangeItems);
                break;
            case ("fs-open", "request"):
                AssertRequest(frame, expect, canonical, f => UniSyscall.DecodeFsOpenRequest(f), f => UniSyscall.EncodeFsOpenRequest(f), AssertFsOpenArgv);
                break;
            case ("fs-open", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsOpenResult(f),
                    f => UniSyscall.EncodeFsOpenResult(f),
                    (fd, e) => Assert.Equal(e.GetProperty("fd").GetUInt32(), fd));
                break;
            case ("fs-close", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsCloseRequest(f),
                    f => UniSyscall.EncodeFsCloseRequest(f),
                    (fd, e) => Assert.Equal(e.GetProperty("fd").GetUInt32(), fd));
                break;
            case ("fs-close", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeFsCloseResult(f), f => UniSyscall.EncodeFsCloseResult(f));
                break;
            case ("fs-read", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsReadRequest(f),
                    a => UniSyscall.EncodeFsReadRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        Assert.Equal(e.GetProperty("fd").GetUInt32(), a.Item1);
                        Assert.Equal(e.GetProperty("len").GetUInt32(), a.Item2);
                    });
                break;
            case ("fs-read", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsReadResult(f),
                    f => UniSyscall.EncodeFsReadResult(f),
                    (data, e) => Assert.Equal(Hex(e.GetProperty("data_hex")), data));
                break;
            case ("fs-write", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsWriteRequest(f),
                    a => UniSyscall.EncodeFsWriteRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        Assert.Equal(e.GetProperty("fd").GetUInt32(), a.Item1);
                        Assert.Equal(Hex(e.GetProperty("data_hex")), a.Item2);
                    });
                break;
            case ("fs-write", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsWriteResult(f),
                    f => UniSyscall.EncodeFsWriteResult(f),
                    (written, e) => Assert.Equal(e.GetProperty("written").GetUInt32(), written));
                break;
            case ("fs-pread", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsPreadRequest(f),
                    a => UniSyscall.EncodeFsPreadRequest(a.Item1, a.Item2, a.Item3),
                    (a, e) =>
                    {
                        Assert.Equal(e.GetProperty("fd").GetUInt32(), a.Item1);
                        Assert.Equal(U64(e.GetProperty("offset")), a.Item2);
                        Assert.Equal(e.GetProperty("len").GetUInt32(), a.Item3);
                    });
                break;
            case ("fs-pread", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsPreadResult(f),
                    f => UniSyscall.EncodeFsPreadResult(f),
                    (data, e) => Assert.Equal(Hex(e.GetProperty("data_hex")), data));
                break;
            case ("fs-pwrite", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsPwriteRequest(f),
                    a => UniSyscall.EncodeFsPwriteRequest(a.Item1, a.Item2, a.Item3),
                    (a, e) =>
                    {
                        Assert.Equal(e.GetProperty("fd").GetUInt32(), a.Item1);
                        Assert.Equal(U64(e.GetProperty("offset")), a.Item2);
                        Assert.Equal(Hex(e.GetProperty("data_hex")), a.Item3);
                    });
                break;
            case ("fs-pwrite", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeFsPwriteResult(f), f => UniSyscall.EncodeFsPwriteResult(f));
                break;
            case ("fs-lseek", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsLseekRequest(f),
                    a => UniSyscall.EncodeFsLseekRequest(a.Item1, a.Item2, a.Item3),
                    (a, e) =>
                    {
                        Assert.Equal(e.GetProperty("fd").GetUInt32(), a.Item1);
                        Assert.Equal(I64(e.GetProperty("offset")), a.Item2);
                        Assert.Equal(e.GetProperty("whence").GetUInt32(), a.Item3);
                    });
                break;
            case ("fs-lseek", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsLseekResult(f),
                    f => UniSyscall.EncodeFsLseekResult(f),
                    (position, e) => Assert.Equal(U64(e.GetProperty("position")), position));
                break;
            case ("fs-fstat", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsFstatRequest(f),
                    f => UniSyscall.EncodeFsFstatRequest(f),
                    (fd, e) => Assert.Equal(e.GetProperty("fd").GetUInt32(), fd));
                break;
            case ("fs-fstat", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeFsFstatResult(f), f => UniSyscall.EncodeFsFstatResult(f), AssertFsStat);
                break;
            case ("fs-stat", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsStatRequest(f),
                    a => UniSyscall.EncodeFsStatRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(e.GetProperty("path").GetString(), a.Item2);
                    });
                break;
            case ("fs-stat", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeFsStatResult(f), f => UniSyscall.EncodeFsStatResult(f), AssertFsStat);
                break;
            case ("fs-fsync", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsFsyncRequest(f),
                    f => UniSyscall.EncodeFsFsyncRequest(f),
                    (fd, e) => Assert.Equal(e.GetProperty("fd").GetUInt32(), fd));
                break;
            case ("fs-fsync", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeFsFsyncResult(f), f => UniSyscall.EncodeFsFsyncResult(f));
                break;
            case ("fs-readdir", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeFsReaddirRequest(f),
                    a => UniSyscall.EncodeFsReaddirRequest(a.Item1, a.Item2),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(e.GetProperty("path").GetString(), a.Item2);
                    });
                break;
            case ("fs-readdir", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeFsReaddirResult(f), f => UniSyscall.EncodeFsReaddirResult(f), AssertReaddirEntries);
                break;
            case ("relation-get", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeRelationGetRequest(f),
                    a => UniSyscall.EncodeRelationGetRequest(a.Item1, a.Item2, a.Item3, a.Item4),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(e.GetProperty("table").GetString(), a.Item2);
                        AssertRelationColumns(a.Item3, e.GetProperty("key"));
                        AssertRelationSelect(a.Item4, e.GetProperty("select"));
                    });
                break;
            case ("relation-get", "response"):
                AssertResponse(frame, expect, canonical, f => UniSyscall.DecodeRelationGetResult(f), f => UniSyscall.EncodeRelationGetResult(f), AssertRelationRow);
                break;
            case ("relation-update", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeRelationUpdateRequest(f),
                    a => UniSyscall.EncodeRelationUpdateRequest(a.Item1, a.Item2, a.Item3, a.Item4, a.Item5),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(e.GetProperty("table").GetString(), a.Item2);
                        AssertRelationColumns(a.Item3, e.GetProperty("key"));
                        AssertRelationColumns(a.Item4, e.GetProperty("values"));
                        AssertRelationDeltas(a.Item5, e.GetProperty("deltas"));
                    });
                break;
            case ("relation-update", "response"):
                AssertResponse(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeRelationUpdateResult(f),
                    f => UniSyscall.EncodeRelationUpdateResult(f),
                    (affected, e) => Assert.Equal(U64(e.GetProperty("affected")), affected));
                break;
            case ("relation-insert", "request"):
                AssertRequest(
                    frame,
                    expect,
                    canonical,
                    f => UniSyscall.DecodeRelationInsertRequest(f),
                    a => UniSyscall.EncodeRelationInsertRequest(a.Item1, a.Item2, a.Item3, a.Item4),
                    (a, e) =>
                    {
                        AssertOid(a.Item1, e.GetProperty("oid"));
                        Assert.Equal(e.GetProperty("table").GetString(), a.Item2);
                        AssertRelationColumns(a.Item3, e.GetProperty("key"));
                        AssertRelationColumns(a.Item4, e.GetProperty("values"));
                    });
                break;
            case ("relation-insert", "response"):
                AssertUnitResponse(frame, expect, canonical, f => UniSyscall.DecodeRelationInsertResult(f), f => UniSyscall.EncodeRelationInsertResult(f));
                break;
            default:
                throw new InvalidDataException($"unsupported corpus vector {kindName}/{direction}");
        }
    }

    // ---- sidecar scalar conventions ----

    private static ulong U64(JsonElement element) =>
        ulong.Parse(element.GetString()!, CultureInfo.InvariantCulture);

    private static long I64(JsonElement element) =>
        long.Parse(element.GetString()!, CultureInfo.InvariantCulture);

    private static byte[] Hex(JsonElement element) => Convert.FromHexString(element.GetString()!);

    private static void AssertOid(UniOid actual, JsonElement expect)
    {
        Assert.Equal(U64(expect.GetProperty("h")), actual.H);
        Assert.Equal(U64(expect.GetProperty("l")), actual.L);
    }

    // ---- frame-shape drivers ----

    /// Decodes a request frame, asserts it against the sidecar expect object
    /// and — for canonical frames — pins the encoder by requiring the
    /// re-encoded arguments to reproduce the committed frame byte for byte.
    private static void AssertRequest<T>(
        byte[] frame,
        JsonElement expect,
        bool canonical,
        Func<byte[], T> decode,
        Func<T, byte[]> encode,
        Action<T, JsonElement> assert)
    {
        var decoded = decode(frame);
        assert(decoded, expect);
        if (canonical)
        {
            Assert.Equal(frame, encode(decoded));
        }
    }

    /// Decodes an ok response frame and asserts it against the sidecar
    /// expect object; for canonical frames a decode → re-encode → decode
    /// roundtrip must stay semantically equal.
    private static void AssertResponse<T>(
        byte[] frame,
        JsonElement expect,
        bool canonical,
        Func<byte[], SyscallResult<T>> decode,
        Func<SyscallResult<T>, byte[]> encode,
        Action<T?, JsonElement> assert)
    {
        var result = decode(frame);
        Assert.True(result.IsOk);
        assert(result.Value, expect);
        if (canonical)
        {
            var again = decode(encode(result));
            Assert.True(again.IsOk);
            assert(again.Value, expect);
        }
    }

    /// Decodes a unit (`{"unit": true}`) response frame; the canonical
    /// roundtrip re-encodes the unit frame and decodes it again.
    private static void AssertUnitResponse(
        byte[] frame,
        JsonElement expect,
        bool canonical,
        Func<byte[], UniError?> decode,
        Func<UniError?, byte[]> encode)
    {
        Assert.True(expect.GetProperty("unit").GetBoolean());
        Assert.Null(decode(frame));
        if (canonical)
        {
            Assert.Null(decode(encode(null)));
        }
    }

    /// Decodes an error response frame and asserts the carried UniError
    /// field by field (the caller location is not byte-restorable, so err
    /// frames get no re-encode pin).
    private static void AssertErrorResponse<T>(
        byte[] frame,
        JsonElement expect,
        Func<byte[], SyscallResult<T>> decode)
    {
        var result = decode(frame);
        Assert.True(result.IsErr);
        var error = result.Error!.Value;
        Assert.Equal(expect.GetProperty("err_code").GetUInt32(), error.ErrCode);
        Assert.Equal(expect.GetProperty("err_msg").GetString(), error.ErrMsg);
        Assert.Equal(expect.GetProperty("err_src").GetString(), error.ErrSrc);
        Assert.Equal(expect.GetProperty("err_loc").GetString(), error.ErrLoc);
        Assert.Equal(Hex(expect.GetProperty("err_details_hex")), error.ErrDetails);
    }

    // ---- per-kind expect assertions ----

    private static void AssertQueryArgv(UniQueryArgv argv, JsonElement expect)
    {
        AssertOid(argv.Oid, expect.GetProperty("oid"));
        Assert.Equal(expect.GetProperty("sql").GetString(), argv.Query.SqlString);
        AssertParams(argv.ParamList, expect);
    }

    private static void AssertCommandArgv(UniCommandArgv argv, JsonElement expect)
    {
        AssertOid(argv.Oid, expect.GetProperty("oid"));
        Assert.Equal(expect.GetProperty("sql").GetString(), argv.Command.SqlString);
        AssertParams(argv.ParamList, expect);
    }

    private static void AssertParams(UniSqlParam paramList, JsonElement expect)
    {
        var count = expect.GetProperty("params").GetArrayLength();
        Assert.Equal(count, paramList.Params.Count);
        if (count != 0)
        {
            throw new InvalidDataException("corpus sql params cell schema not supported yet");
        }
    }

    private static void AssertQueryResult(UniQueryResult value, JsonElement expect)
    {
        Assert.Equal(expect.GetProperty("record_name").GetString(), value.TupleDesc.RecordName);
        var fields = expect.GetProperty("fields");
        Assert.Equal(fields.GetArrayLength(), value.TupleDesc.RecordFields.Count);
        var i = 0;
        foreach (var field in fields.EnumerateArray())
        {
            var actual = value.TupleDesc.RecordFields[i++];
            Assert.Equal(field.GetProperty("field_name").GetString(), actual.FieldName);
            AssertDataType(field.GetProperty("field_type").GetString()!, actual.FieldType);
            var attrs = field.GetProperty("field_attrs");
            Assert.Equal(attrs.GetArrayLength(), actual.FieldAttrs.Count);
            var j = 0;
            foreach (var attr in attrs.EnumerateArray())
            {
                var actualAttr = actual.FieldAttrs[j++];
                Assert.Equal(attr.GetProperty("attr_name").GetString(), actualAttr.AttrName);
                Assert.Equal(attr.GetProperty("attr_value").GetString(), actualAttr.AttrValue);
            }
        }

        Assert.Equal(expect.GetProperty("eof").GetBoolean(), value.ResultSet.Eof);
        var rows = expect.GetProperty("rows");
        Assert.Equal(rows.GetArrayLength(), value.ResultSet.RowSet.Count);
        if (rows.GetArrayLength() != 0)
        {
            throw new InvalidDataException("corpus query row cell schema not supported yet");
        }

        Assert.Equal(Hex(expect.GetProperty("cursor_hex")), value.ResultSet.Cursor);
    }

    /// The sidecar renders a field type as `scalar(<wit-name>)`; only the
    /// scalar case appears in the corpora so far.
    private static void AssertDataType(string expected, UniDataType actual)
    {
        if (expected.StartsWith("scalar(", StringComparison.Ordinal) && expected.EndsWith(')'))
        {
            var scalar = Enum.Parse<UniScalar>(expected["scalar(".Length..^1], ignoreCase: true);
            var actualScalar = Assert.IsType<UniDataTypeScalar>(actual);
            Assert.Equal(scalar, actualScalar.Inner);
            return;
        }

        throw new InvalidDataException($"unsupported corpus field_type '{expected}'");
    }

    private static void AssertCommandResult(UniCommandResult result, JsonElement expect)
    {
        Assert.Equal(U64(expect.GetProperty("affected_rows")), result.AffectedRows);
    }

    private static void AssertRangeItems(List<(byte[], byte[])>? items, JsonElement expect)
    {
        var expected = expect.GetProperty("items");
        var actual = items!;
        Assert.Equal(expected.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var item in expected.EnumerateArray())
        {
            Assert.Equal(Hex(item.GetProperty("key_hex")), actual[i].Item1);
            Assert.Equal(Hex(item.GetProperty("value_hex")), actual[i].Item2);
            i++;
        }
    }

    private static void AssertFsOpenArgv(UniFsOpenArgv argv, JsonElement expect)
    {
        AssertOid(argv.Session, expect.GetProperty("session"));
        AssertOid(argv.Oid, expect.GetProperty("oid"));
        Assert.Equal(expect.GetProperty("path").GetString(), argv.Path);
        Assert.Equal(expect.GetProperty("flags").GetUInt32(), argv.Flags);
    }

    private static void AssertFsStat(UniFsStat value, JsonElement expect)
    {
        var s = expect.GetProperty("stat");
        AssertOid(value.Oid, s.GetProperty("oid"));
        Assert.Equal(U64(s.GetProperty("generation")), value.Generation);
        Assert.Equal(s.GetProperty("entry").GetString(), value.Entry);
        Assert.Equal(U64(s.GetProperty("length")), value.Length);
        Assert.Equal(s.GetProperty("state").GetUInt32(), value.State);
    }

    private static void AssertReaddirEntries(List<UniFsDirent>? entries, JsonElement expect)
    {
        var expected = expect.GetProperty("entries");
        var actual = entries!;
        Assert.Equal(expected.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var entry in expected.EnumerateArray())
        {
            var a = actual[i++];
            Assert.Equal(entry.GetProperty("name").GetString(), a.Name);
            Assert.Equal(entry.GetProperty("is_dir").GetBoolean(), a.IsDir);
            Assert.Equal(U64(entry.GetProperty("length")), a.Length);
        }
    }

    private static void AssertRelationColumns(List<(ulong, byte[])> actual, JsonElement expect)
    {
        Assert.Equal(expect.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var column in expect.EnumerateArray())
        {
            Assert.Equal(U64(column.GetProperty("attr")), actual[i].Item1);
            Assert.Equal(Hex(column.GetProperty("datum_hex")), actual[i].Item2);
            i++;
        }
    }

    private static void AssertRelationSelect(List<ulong> actual, JsonElement expect)
    {
        Assert.Equal(expect.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var attr in expect.EnumerateArray())
        {
            Assert.Equal(U64(attr), actual[i++]);
        }
    }

    private static void AssertRelationDeltas(List<(ulong, byte, byte[])> actual, JsonElement expect)
    {
        Assert.Equal(expect.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var delta in expect.EnumerateArray())
        {
            Assert.Equal(U64(delta.GetProperty("attr")), actual[i].Item1);
            Assert.Equal(delta.GetProperty("op").GetByte(), actual[i].Item2);
            Assert.Equal(Hex(delta.GetProperty("datum_hex")), actual[i].Item3);
            i++;
        }
    }

    private static void AssertRelationRow(List<byte[]?>? row, JsonElement expect)
    {
        var expected = expect.GetProperty("row");
        if (expected.ValueKind == JsonValueKind.Null)
        {
            Assert.Null(row);
            return;
        }

        var actual = row!;
        Assert.Equal(expected.GetArrayLength(), actual.Count);
        var i = 0;
        foreach (var cell in expected.EnumerateArray())
        {
            if (cell.ValueKind == JsonValueKind.Null)
            {
                Assert.Null(actual[i]);
            }
            else
            {
                Assert.Equal(Hex(cell), actual[i]);
            }

            i++;
        }
    }
}
