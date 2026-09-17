#nullable enable

using System.Buffers;
using MessagePack;
using mududb.types;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Bug-1 regression pin: the generated universal scalar tags must match the
/// host wire format (corrected `uni-scalar.wit` / `uni-scalar-value.wit` /
/// `uni-data-type.wit`, mirrored by the Rust host `uni_scalar_value.rs`).
/// The retired hand-written DTOs declared `I64 = 8` and `String = 12`; the
/// host encodes `I64 = 9` and `String = 14`.
/// </summary>
public class UniScalarTagTests
{
    [Fact]
    public void UniScalarDiscriminantsMatchHost()
    {
        Assert.Equal(0, (int)UniScalar.Bool);
        Assert.Equal(7, (int)UniScalar.U64);
        Assert.Equal(8, (int)UniScalar.U128);
        Assert.Equal(9, (int)UniScalar.I64);
        Assert.Equal(10, (int)UniScalar.I128);
        Assert.Equal(11, (int)UniScalar.F32);
        Assert.Equal(12, (int)UniScalar.F64);
        Assert.Equal(13, (int)UniScalar.Char);
        Assert.Equal(14, (int)UniScalar.String);
        Assert.Equal(15, (int)UniScalar.Blob);
        Assert.Equal(16, (int)UniScalar.Numeric);
        Assert.Equal(17, (int)UniScalar.Date);
        Assert.Equal(18, (int)UniScalar.Time);
        Assert.Equal(19, (int)UniScalar.Timestamp);
        Assert.Equal(20, (int)UniScalar.TimestampTz);
    }

    [Fact]
    public void UniScalarValueKindDiscriminantsMatchHost()
    {
        Assert.Equal(8, (int)UniScalarValueKind.U128);
        Assert.Equal(9, (int)UniScalarValueKind.I64);
        Assert.Equal(10, (int)UniScalarValueKind.I128);
        Assert.Equal(14, (int)UniScalarValueKind.String);
        Assert.Equal(20, (int)UniScalarValueKind.TimestampTz);
    }

    [Fact]
    public void UniDataTypeKindBoxIsHostTag8()
    {
        Assert.Equal(8, (int)UniDataTypeKind.Box);
    }

    [Fact]
    public void ScalarValueWireTagsMatchHost()
    {
        // Variant wire form is `[tag, inner]`; the tag byte is the pin.
        Assert.Equal([0x92, 0x09, 0x2A], Encode(new UniScalarValueI64 { Inner = 42 }));
        Assert.Equal([0x92, 0x0E, 0xA2, 0x68, 0x69], Encode(new UniScalarValueString { Inner = "hi" }));
        Assert.Equal([0x92, 0x08, 0x90], Encode(new UniScalarValueU128 { Inner = [] }));
        Assert.Equal([0x92, 0x0A, 0x90], Encode(new UniScalarValueI128 { Inner = [] }));
        Assert.Equal(
            [0x92, 0x14, 0xB0, 0x32, 0x30, 0x32, 0x36, 0x2D, 0x30, 0x31, 0x2D, 0x30, 0x32, 0x20, 0x30, 0x30, 0x3A, 0x30, 0x30],
            Encode(new UniScalarValueTimestampTz { Inner = "2026-01-02 00:00" }));
    }

    [Fact]
    public void DataTypeBoxWireTagIs8()
    {
        // `box` wraps an inner data type: [8, [scalar-tag, 0]] round-trips
        // through the generated formatter.
        var box = new UniDataTypeBox
        {
            Inner = new UniDataTypeScalar { Inner = UniScalar.I64 },
        };
        var bytes = Encode(box);
        Assert.Equal(0x92, bytes[0]);
        Assert.Equal(0x08, bytes[1]);
        var reader = new MessagePackReader(bytes);
        var decoded = new UniDataTypeFormatter().Deserialize(ref reader, null!);
        var decodedBox = Assert.IsType<UniDataTypeBox>(decoded);
        Assert.Equal(UniScalar.I64, UniDataTypeScalar.AsScalar(decodedBox.Inner).Inner);
    }

    private static byte[] Encode<T>(T value)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        switch (value)
        {
            case UniScalarValue scalarValue:
                new UniScalarValueFormatter().Serialize(ref writer, scalarValue, null!);
                break;
            case UniDataType dataType:
                new UniDataTypeFormatter().Serialize(ref writer, dataType, null!);
                break;
            default:
                throw new global::System.NotSupportedException($"unexpected value type {typeof(T)}");
        }

        writer.Flush();
        return buffer.WrittenSpan.ToArray();
    }
}
