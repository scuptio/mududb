#nullable enable

using System.Text.Json;
using MessagePack;
using mududb.codec;
using mududb.types;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Lenient-decode interop proof: the C# SDK decoder against the hand-built
/// NON-canonical MSSP v1 frames of
/// `crates/db-kernel/testing/fixtures/golden/v1/lenient_decode_v1.bin`
/// (generated once by the `generate_golden_v1_fixtures` test in the
/// `testing` crate). Each of the 8 segments is a complete MSSP frame (16-byte
/// header + MessagePack body) that the canonical encoder could never emit;
/// the JSON sidecar `lenient_decode_v1.json` carries the semantic
/// expectations, one `vectors` entry per segment.
///
/// Every vector is decoded with the request or result codec selected by its
/// `direction` and compared against the sidecar `expect` object by
/// <see cref="MsspCorpus.AssertVector"/>. There is no re-encode pin: the
/// bytes are intentionally non-canonical. The covered categories are
/// int-width-widening, int-unsigned-marker, int-wide-negative,
/// record-key-order, record-unknown-field, map-noninteger-key,
/// request-missing-param and record-missing-fields. This test only reads the
/// fixture — it never writes it.
/// </summary>
public class LenientDecodeCorpusTests
{
    [Fact]
    public void LenientVectorsDecodeToTheSidecarExpectations()
    {
        var segments = MsspCorpus.UnpackSegments(File.ReadAllBytes(MsspCorpus.FixturePath("lenient_decode_v1.bin")));
        using var sidecar = JsonDocument.Parse(File.ReadAllBytes(MsspCorpus.FixturePath("lenient_decode_v1.json")));
        var vectors = sidecar.RootElement.GetProperty("vectors");
        Assert.Equal(vectors.GetArrayLength(), segments.Count);

        var kinds = new HashSet<string>();
        var index = 0;
        foreach (var vector in vectors.EnumerateArray())
        {
            Assert.Equal(index, vector.GetProperty("index").GetInt32());
            kinds.Add(vector.GetProperty("kind").GetString()!);
            MsspCorpus.AssertVector(segments[index], vector, canonical: false);
            index++;
        }

        Assert.Equal(segments.Count, kinds.Count);
    }

    // ---- supplementary pins not representable in the corpus ----

    [Fact]
    public void MissingRecordFieldsDecodeToTypeDefaults()
    {
        byte[] emptyMap = [0x80];

        var errorReader = new MessagePackReader(emptyMap);
        var error = new UniErrorFormatter().Deserialize(ref errorReader, null!);
        Assert.Equal(0u, error.ErrCode);
        Assert.Equal(string.Empty, error.ErrMsg);
        Assert.Equal(string.Empty, error.ErrSrc);
        Assert.Equal(string.Empty, error.ErrLoc);
        Assert.Empty(error.ErrDetails);

        // A missing variant-typed field decodes to the default (first) case
        // instance, never null: UniDataValue.Scalar(UniScalarValue.Bool(false)).
        var fieldReader = new MessagePackReader(emptyMap);
        var field = new UniDataValueFieldFormatter().Deserialize(ref fieldReader, null!);
        Assert.Equal(string.Empty, field.FieldName);
        var scalar = Assert.IsType<UniDataValueScalar>(field.FieldValue);
        var boolean = Assert.IsType<UniScalarValueBool>(scalar.Inner);
        Assert.False(boolean.Inner);
    }

    [Fact]
    public void MissingRequestArgumentsDecodeToTypeDefaults()
    {
        // `get(oid, key)` with only parameter 2 present: {2: bin "k"} — the
        // missing oid argument decodes to the zeroed record, not an error.
        byte[] partial = [0x81, 0x02, 0xC4, 0x01, 0x6B];
        var (defaultOid, partialKey) = SyscallPayload.DecodeRequestBody<UniOid, byte[]>(partial);
        Assert.Equal(default(UniOid), defaultOid);
        Assert.Equal("k"u8.ToArray(), partialKey);

        // A missing byte[] argument decodes to empty, not null.
        var missingBlob = SyscallPayload.DecodeRequestBody<uint, byte[]>(new byte[] { 0x81, 0x01, 0x03 });
        Assert.Equal(3u, missingBlob.Item1);
        Assert.Empty(missingBlob.Item2);
    }
}
