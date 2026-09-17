#nullable enable

using System.Buffers;
using System.Globalization;
using System.Text.Json;
using MessagePack;
using Xunit;

namespace mududb.tests;

/// <summary>
/// MessagePack primitive alignment proof: the C# SDK (MessagePack-CSharp)
/// against the shared cross-language primitive corpus produced by rmp_serde
/// 1.3.1.
///
/// The corpus is
/// `crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin`
/// (generated once by the `generate_mp_primitives_v1` test in the `testing`
/// crate). It packs 44 single-value MessagePack encodings as consecutive
/// segments, each prefixed with its big-endian u32 byte length. The JSON
/// sidecar `mp_primitives_v1.json` lists `(index, kind, value|len)` per
/// segment so each expected value can be rebuilt here.
///
/// For every vector this test asserts that MessagePack-CSharp encodes the
/// value to exactly the golden bytes AND decodes the golden bytes back to
/// the value. rmp_serde is authoritative; where MessagePack-CSharp's
/// type-driven writer would diverge (it does not for the vector set: its
/// integer writes are minimal-width by value, matching rmp_serde), that
/// would be a codec finding, not a corpus change.
/// </summary>
public class MpPrimitiveCorpusTests
{
    /// Locates the fixture directory by walking up from the test assembly,
    /// so the test reads the committed files in place.
    private static string FixturePath(string name)
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
            $"primitive corpus {name} not found above {AppContext.BaseDirectory}; " +
            "run generate_mp_primitives_v1 in the testing crate");
    }

    /// Splits the corpus bytes into the individual value segments.
    private static List<byte[]> UnpackSegments(byte[] bytes)
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

    /// Pattern bytes for a `bin` vector: byte i = i mod 251.
    private static byte[] PatternBin(int length)
    {
        var bytes = new byte[length];
        for (var i = 0; i < length; i++)
        {
            bytes[i] = (byte)(i % 251);
        }

        return bytes;
    }

    /// Asserts `encoded` equals the golden segment and that the segment is a
    /// single self-contained MessagePack value (no trailing bytes).
    private static void AssertSegment(byte[] segment, byte[] encoded)
    {
        Assert.Equal(segment, encoded);
        var reader = new MessagePackReader(segment);
        reader.Skip();
        Assert.Equal(segment.Length, (int)reader.Consumed);
    }

    [Fact]
    public void PrimitiveCorpusMatchesCSharpCodec()
    {
        var segments = UnpackSegments(File.ReadAllBytes(FixturePath("mp_primitives_v1.bin")));
        using var sidecar = JsonDocument.Parse(File.ReadAllBytes(FixturePath("mp_primitives_v1.json")));
        var vectors = sidecar.RootElement.GetProperty("vectors");
        Assert.Equal(segments.Count, vectors.GetArrayLength());

        var index = 0;
        foreach (var vector in vectors.EnumerateArray())
        {
            Assert.Equal(index, vector.GetProperty("index").GetInt32());
            var segment = segments[index];
            var kind = vector.GetProperty("kind").GetString();
            var hasValue = vector.TryGetProperty("value", out var value);
            var hasLen = vector.TryGetProperty("len", out var len);
            switch (kind)
            {
                case "u64":
                    {
                        var expected = ulong.Parse(value.GetString()!, CultureInfo.InvariantCulture);
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<ulong>(segment));
                        break;
                    }

                case "i64":
                    {
                        var expected = long.Parse(value.GetString()!, CultureInfo.InvariantCulture);
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<long>(segment));
                        break;
                    }

                case "f32":
                    {
                        var expected = float.Parse(value.GetString()!, CultureInfo.InvariantCulture);
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<float>(segment));
                        break;
                    }

                case "f64":
                    {
                        var expected = double.Parse(value.GetString()!, CultureInfo.InvariantCulture);
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<double>(segment));
                        break;
                    }

                case "nil":
                    {
                        Assert.False(hasValue);
                        var buffer = new ArrayBufferWriter<byte>();
                        var writer = new MessagePackWriter(buffer);
                        writer.WriteNil();
                        writer.Flush();
                        AssertSegment(segment, buffer.WrittenSpan.ToArray());
                        var reader = new MessagePackReader(segment);
                        Assert.True(reader.TryReadNil());
                        break;
                    }

                case "bool":
                    {
                        var expected = value.GetBoolean();
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<bool>(segment));
                        break;
                    }

                case "str":
                    {
                        var expected = hasValue ? value.GetString()! : new string('a', len.GetInt32());
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<string>(segment));
                        break;
                    }

                case "bin":
                    {
                        Assert.False(hasValue);
                        var expected = PatternBin(len.GetInt32());
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<byte[]>(segment));
                        break;
                    }

                case "array":
                    {
                        Assert.False(hasValue);
                        var expected = Enumerable.Range(0, len.GetInt32()).Select(i => (ulong)i).ToArray();
                        AssertSegment(segment, MessagePackSerializer.Serialize(expected));
                        Assert.Equal(expected, MessagePackSerializer.Deserialize<ulong[]>(segment));
                        break;
                    }

                case "combo":
                    {
                        // The fixed nested vector [u64 42, str "hi", bin 0xDEADBEEF].
                        var buffer = new ArrayBufferWriter<byte>();
                        var writer = new MessagePackWriter(buffer);
                        writer.WriteArrayHeader(3);
                        writer.Write(42UL);
                        writer.Write("hi");
                        writer.Write(new byte[] { 0xDE, 0xAD, 0xBE, 0xEF });
                        writer.Flush();
                        AssertSegment(segment, buffer.WrittenSpan.ToArray());
                        var reader = new MessagePackReader(segment);
                        Assert.Equal(3, reader.ReadArrayHeader());
                        Assert.Equal(42UL, reader.ReadUInt64());
                        Assert.Equal("hi", reader.ReadString());
                        Assert.Equal(new byte[] { 0xDE, 0xAD, 0xBE, 0xEF }, reader.ReadBytes()!.Value.ToArray());
                        break;
                    }

                default:
                    throw new InvalidDataException($"unknown corpus vector kind '{kind}' at index {index}");
            }

            index++;
        }
    }

    /// <summary>
    /// Pins the decode-side rules guests must mirror from rmp_serde: reading
    /// a u64 rejects negative encodings, and f32/f64 decode leniently across
    /// the 0xCA/0xCB markers in both directions.
    /// </summary>
    [Fact]
    public void PinnedDecodeRulesMatchRmpSerde()
    {
        // 0xFF is the negfixint encoding of -1; rmp_serde from_slice::<u64>
        // errors on it and MessagePack-CSharp must reject it too.
        Assert.Throws<MessagePackSerializationException>(
            () => MessagePackSerializer.Deserialize<ulong>(new byte[] { 0xFF }));

        // f32 reads from an f64 (0xCB) encoding and vice versa.
        Assert.Equal(1.5f, MessagePackSerializer.Deserialize<float>(MessagePackSerializer.Serialize(1.5d)));
        Assert.Equal(1.5d, MessagePackSerializer.Deserialize<double>(MessagePackSerializer.Serialize(1.5f)));
    }
}
