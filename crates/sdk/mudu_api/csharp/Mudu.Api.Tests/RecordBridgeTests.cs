#nullable enable

using System.Buffers;
using MessagePack;
using MessagePack.Formatters;
using mududb.codec;
using mududb.types;
using Xunit;

namespace mududb.tests;

/// <summary>
/// Tests for <see cref="RecordBridge"/>: the hand-written transcoder between
/// the host's uni-data-value record-case envelope and the integer-keyed
/// MessagePack map the mgen-generated record formatters consume.
///
/// The Address/Profile DTOs below hand-mirror the mgen C# record template
/// output (`record.cs.jinja` + `codec_expr_cs.rs`) for this WIT shape:
///
///   record address { city: string, zip: string }
///   record profile {
///       display-name: string,   // 1
///       level: u32,             // 2  (i32 on the host wire)
///       vip: bool,              // 3  (i32 0/1 on the host wire)
///       home: option&lt;address&gt;,  // 4 (omitted when null: gap-fill position)
///       tags: list&lt;string&gt;,     // 5
///       score: f64,             // 6
///       balance: i64,           // 7
///       avatar: blob,           // 8  (MessagePack bin)
///       nickname: option&lt;string&gt;,// 9 (omitted when null: trailing shrink)
///   }
/// </summary>
public class RecordBridgeTests
{
    // ---- mgen-template mirror DTOs ----

    public struct Address
    {
        [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
        public Address()
        {
            City = string.Empty;
            Zip = string.Empty;
        }

        public required string City { get; set; }

        public required string Zip { get; set; }
    }

    public class AddressFormatter : IMessagePackFormatter<Address>
    {
        public void Serialize(ref MessagePackWriter writer, Address value, MessagePackSerializerOptions options)
        {
            writer.WriteMapHeader(2);
            writer.Write(1);
            writer.Write(value.City);
            writer.Write(2);
            writer.Write(value.Zip);
        }

        public Address Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
        {
            var count = reader.ReadMapHeader();
            var value = new Address();
            for (var i = 0; i < count; i++)
            {
                if (reader.NextMessagePackType != MessagePackType.Integer)
                {
                    reader.Skip();
                    reader.Skip();
                    continue;
                }

                switch (reader.ReadInt64())
                {
                    case 1:
                        value.City = reader.ReadString()!;
                        break;
                    case 2:
                        value.Zip = reader.ReadString()!;
                        break;
                    default:
                        reader.Skip();
                        break;
                }
            }

            return value;
        }
    }

    public struct Profile
    {
        [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
        public Profile()
        {
            DisplayName = string.Empty;
            Level = 0;
            Vip = false;
            Home = null;
            Tags = [];
            Score = 0;
            Balance = 0;
            Avatar = [];
            Nickname = null;
        }

        public required string DisplayName { get; set; }

        public required uint Level { get; set; }

        public required bool Vip { get; set; }

        public required Address? Home { get; set; }

        public required List<string> Tags { get; set; }

        public required double Score { get; set; }

        public required long Balance { get; set; }

        public required byte[] Avatar { get; set; }

        public required string? Nickname { get; set; }
    }

    public class ProfileFormatter : IMessagePackFormatter<Profile>
    {
        public void Serialize(ref MessagePackWriter writer, Profile value, MessagePackSerializerOptions options)
        {
            // WIT option fields are omitted when null (proto3 presence
            // semantics); decode treats a missing key as the default.
            writer.WriteMapHeader(9 - (value.Home is not null ? 0 : 1) - (value.Nickname is not null ? 0 : 1));
            writer.Write(1);
            writer.Write(value.DisplayName);
            writer.Write(2);
            writer.Write(value.Level);
            writer.Write(3);
            writer.Write(value.Vip);
            if (value.Home is not null)
            {
                writer.Write(4);
                new AddressFormatter().Serialize(ref writer, value.Home.Value, options);
            }

            writer.Write(5);
            writer.WriteArrayHeader(value.Tags.Count);
            foreach (var tag in value.Tags)
            {
                writer.Write(tag);
            }

            writer.Write(6);
            writer.Write(value.Score);
            writer.Write(7);
            writer.Write(value.Balance);
            writer.Write(8);
            writer.Write(value.Avatar);
            if (value.Nickname is not null)
            {
                writer.Write(9);
                writer.Write(value.Nickname);
            }
        }

        public Profile Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
        {
            var count = reader.ReadMapHeader();
            var value = new Profile();
            for (var i = 0; i < count; i++)
            {
                if (reader.NextMessagePackType != MessagePackType.Integer)
                {
                    reader.Skip();
                    reader.Skip();
                    continue;
                }

                switch (reader.ReadInt64())
                {
                    case 1:
                        value.DisplayName = reader.ReadString()!;
                        break;
                    case 2:
                        value.Level = reader.ReadUInt32();
                        break;
                    case 3:
                        // The emitted mgen read: the host sends bool fields
                        // as i32 (0/1).
                        value.Vip = reader.NextMessagePackType == MessagePackType.Integer
                            ? reader.ReadInt64() switch
                            {
                                0 => false,
                                1 => true,
                                _ => throw new InvalidDataException("MSSP expected a bool encoded as 0/1"),
                            }
                            : reader.ReadBoolean();
                        break;
                    case 4:
                        if (reader.TryReadNil())
                        {
                            value.Home = null;
                        }
                        else
                        {
                            value.Home = new AddressFormatter().Deserialize(ref reader, options);
                        }

                        break;
                    case 5:
                    {
                        var n = reader.ReadArrayHeader();
                        var tags = new List<string>(n);
                        for (var t = 0; t < n; t++)
                        {
                            tags.Add(reader.ReadString()!);
                        }

                        value.Tags = tags;
                        break;
                    }

                    case 6:
                        value.Score = reader.ReadDouble();
                        break;
                    case 7:
                        value.Balance = reader.ReadInt64();
                        break;
                    case 8:
                        value.Avatar = BuffersExtensions.ToArray(reader.ReadBytes()!.Value);
                        break;
                    case 9:
                        value.Nickname = reader.ReadString();
                        break;
                    default:
                        reader.Skip();
                        break;
                }
            }

            return value;
        }
    }

    // ---- envelope builders ----

    private static UniDataValue Scalar(UniScalarValue inner) =>
        new UniDataValueScalar { Inner = inner };

    private static UniDataValue Str(string value) =>
        Scalar(new UniScalarValueString { Inner = value });

    private static UniDataValue I32(int value) =>
        Scalar(new UniScalarValueI32 { Inner = value });

    private static UniDataValue I64(long value) =>
        Scalar(new UniScalarValueI64 { Inner = value });

    private static UniDataValueField Field(string name, UniDataValue value) =>
        new UniDataValueField { FieldName = name, FieldValue = value };

    private static UniDataValue AddressEnvelope(string city, string zip) =>
        new UniDataValueRecord { Inner = [Field("", Str(city)), Field("", Str(zip))] };

    /// The host-style profile envelope: positional fields (names empty,
    /// except the first to prove names are ignored), bool as i32, u32 as
    /// i32, blob as the scalar blob case, a null option as the Null scalar.
    private static UniDataValue ProfileEnvelope(bool withHome) =>
        new UniDataValueRecord
        {
            Inner = new List<UniDataValueField>
            {
                Field("display-name", Str("Ada")),
                Field("", I32(7)),
                Field("", I32(1)),
                Field("", withHome ? AddressEnvelope("Paris", "75001") : Scalar(new UniScalarValueNull { Inner = 0 })),
                Field("", new UniDataValueArray { Inner = [Str("x"), Str("yy")] }),
                Field("", Scalar(new UniScalarValueF64 { Inner = 2.5 })),
                Field("", I64(5000000000)),
                Field("", Scalar(new UniScalarValueBlob { Inner = [1, 2, 255] })),
                Field("", Str("ace")),
            },
        };

    private static Profile SampleProfile(bool withHome, bool withNickname) =>
        new Profile
        {
            DisplayName = "Ada",
            Level = 7,
            Vip = true,
            Home = withHome ? new Address { City = "Paris", Zip = "75001" } : null,
            Tags = ["x", "yy"],
            Score = 2.5,
            Balance = 5000000000,
            Avatar = [1, 2, 255],
            Nickname = withNickname ? "ace" : null,
        };

    // ---- expected wire bytes (independent raw encodings) ----

    private delegate void RefWrite(ref MessagePackWriter w);

    private static byte[] WriteRaw(RefWrite body)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        body(ref writer);
        writer.Flush();
        return buffer.WrittenMemory.ToArray();
    }

    private static void FieldHead(ref MessagePackWriter w)
    {
        w.WriteMapHeader(2);
        w.Write(1);
        w.Write(string.Empty);
        w.Write(2);
    }

    private static void ScalarHead(ref MessagePackWriter w, int tag)
    {
        w.WriteArrayHeader(2);
        w.Write(0); // UniDataValue::Scalar
        w.WriteArrayHeader(2);
        w.Write(tag);
    }

    /// The integer-keyed map the generated ProfileFormatter consumes.
    private static byte[] ExpectedFieldMap(bool withHome) =>
        WriteRaw((ref MessagePackWriter w) =>
        {
            w.WriteMapHeader(9);
            w.Write(1);
            w.Write("Ada");
            w.Write(2);
            w.Write(7);
            w.Write(3);
            w.Write(1);
            w.Write(4);
            if (withHome)
            {
                w.WriteMapHeader(2);
                w.Write(1);
                w.Write("Paris");
                w.Write(2);
                w.Write("75001");
            }
            else
            {
                w.WriteNil();
            }

            w.Write(5);
            w.WriteArrayHeader(2);
            w.Write("x");
            w.Write("yy");
            w.Write(6);
            w.Write(2.5);
            w.Write(7);
            w.Write(5000000000L);
            w.Write(8);
            w.Write(new byte[] { 1, 2, 255 });
            w.Write(9);
            w.Write("ace");
        });

    /// The host record-case envelope: `[2, [ {1: "", 2: datum}, ... ]]`
    /// with `[0, [tag, payload]]` scalar datums.
    private static byte[] ExpectedEnvelope(bool withHome, bool withNickname) =>
        WriteRaw((ref MessagePackWriter w) =>
        {
            var fieldCount = 9 - (withNickname ? 0 : 1);
            w.WriteArrayHeader(2);
            w.Write(2); // UniDataValue::Record
            w.WriteArrayHeader(fieldCount);
            FieldHead(ref w);
            ScalarHead(ref w, 14);
            w.Write("Ada");
            FieldHead(ref w);
            ScalarHead(ref w, 6);
            w.Write(7);
            FieldHead(ref w);
            ScalarHead(ref w, 6);
            w.Write(1);
            FieldHead(ref w);
            if (withHome)
            {
                w.WriteArrayHeader(2);
                w.Write(2); // nested record
                w.WriteArrayHeader(2);
                FieldHead(ref w);
                ScalarHead(ref w, 14);
                w.Write("Paris");
                FieldHead(ref w);
                ScalarHead(ref w, 14);
                w.Write("75001");
            }
            else
            {
                ScalarHead(ref w, 21);
                w.Write((byte)0);
            }

            FieldHead(ref w);
            w.WriteArrayHeader(2);
            w.Write(1); // array
            w.WriteArrayHeader(2);
            ScalarHead(ref w, 14);
            w.Write("x");
            ScalarHead(ref w, 14);
            w.Write("yy");
            FieldHead(ref w);
            ScalarHead(ref w, 12);
            w.Write(2.5);
            FieldHead(ref w);
            ScalarHead(ref w, 9);
            w.Write(5000000000L);
            // A byte array leaves the generated C# encoder as MessagePack
            // bin, which the bridge wraps as the Blob scalar; the envelope
            // serializes the Blob payload as the pinned record-context
            // array of u8.
            FieldHead(ref w);
            ScalarHead(ref w, 15);
            w.WriteArrayHeader(3);
            w.Write((byte)1);
            w.Write((byte)2);
            w.Write((byte)255);
            if (withNickname)
            {
                FieldHead(ref w);
                ScalarHead(ref w, 14);
                w.Write("ace");
            }
        });

    private static byte[] SerializeEnvelope(UniDataValue value) =>
        WriteRaw((ref MessagePackWriter w) => new UniDataValueFormatter().Serialize(ref w, value, null!));

    // ---- checks ----

    [Fact]
    public void RecordFieldValuesIsByteExact()
    {
        Assert.Equal(ExpectedFieldMap(withHome: true), RecordBridge.RecordFieldValues(ProfileEnvelope(true)));
        Assert.Equal(ExpectedFieldMap(withHome: false), RecordBridge.RecordFieldValues(ProfileEnvelope(false)));
    }

    [Fact]
    public void FromValueDecodesTheHostEnvelope()
    {
        var profile = RecordBridge.FromValue(ProfileEnvelope(true), new ProfileFormatter());
        Assert.Equal("Ada", profile.DisplayName);
        Assert.Equal(7u, profile.Level);
        Assert.True(profile.Vip); // crossed as i32 1 (no Bool family)
        Assert.Equal(new Address { City = "Paris", Zip = "75001" }, profile.Home);
        Assert.Equal(["x", "yy"], profile.Tags);
        Assert.Equal(2.5, profile.Score);
        Assert.Equal(5000000000, profile.Balance);
        Assert.Equal([1, 2, 255], profile.Avatar);
        Assert.Equal("ace", profile.Nickname);
    }

    [Fact]
    public void FromValueDefaultsMissingFields()
    {
        var profile = RecordBridge.FromValue(ProfileEnvelope(false), new ProfileFormatter());
        Assert.Null(profile.Home); // the Null scalar at position 4
        Assert.Equal("ace", profile.Nickname);
    }

    [Fact]
    public void RecordFieldValuesRejectsANonRecord()
    {
        Assert.Throws<InvalidDataException>(() => RecordBridge.RecordFieldValues(I32(1)));
    }

    [Fact]
    public void RecordFromFieldValuesIsByteExact()
    {
        var full = RecordBridge.ToValue(SampleProfile(true, true), new ProfileFormatter());
        Assert.Equal(ExpectedEnvelope(true, true), SerializeEnvelope(full));
    }

    [Fact]
    public void RecordFromFieldValuesGapFillsAnOmittedOptionWithNull()
    {
        var datum = RecordBridge.ToValue(SampleProfile(false, true), new ProfileFormatter());
        Assert.Equal(ExpectedEnvelope(false, true), SerializeEnvelope(datum));

        // position 4 is the Null scalar, not a shifted field
        var record = Assert.IsType<UniDataValueRecord>(datum);
        Assert.Equal(9, record.Inner.Count);
        var nullScalar = Assert.IsType<UniScalarValueNull>(
            Assert.IsType<UniDataValueScalar>(record.Inner[3].FieldValue).Inner);
        Assert.Equal(UniScalarValueKind.Null, nullScalar.Kind());
    }

    [Fact]
    public void RecordFromFieldValuesShrinksATrailingOmittedOption()
    {
        var datum = RecordBridge.ToValue(SampleProfile(false, false), new ProfileFormatter());
        Assert.Equal(ExpectedEnvelope(false, false), SerializeEnvelope(datum));
        var record = Assert.IsType<UniDataValueRecord>(datum);
        Assert.Equal(8, record.Inner.Count);
    }

    [Fact]
    public void RecordFromFieldValuesRejectsU64Overflow()
    {
        var bytes = WriteRaw((ref MessagePackWriter w) =>
        {
            w.WriteMapHeader(1);
            w.Write(1);
            w.Write(ulong.MaxValue);
        });
        Assert.Throws<InvalidDataException>(() => RecordBridge.RecordFromFieldValues(bytes));
    }

    [Fact]
    public void RoundTripPreservesTheProfile()
    {
        var profile = SampleProfile(true, true);
        var datum = RecordBridge.ToValue(profile, new ProfileFormatter());
        var decoded = RecordBridge.FromValue(datum, new ProfileFormatter());
        Assert.Equal(profile.DisplayName, decoded.DisplayName);
        Assert.Equal(profile.Level, decoded.Level);
        Assert.Equal(profile.Vip, decoded.Vip);
        Assert.Equal(profile.Home, decoded.Home);
        Assert.Equal(profile.Tags, decoded.Tags);
        Assert.Equal(profile.Score, decoded.Score);
        Assert.Equal(profile.Balance, decoded.Balance);
        Assert.Equal(profile.Avatar, decoded.Avatar);
        Assert.Equal(profile.Nickname, decoded.Nickname);
    }
}
