// Hand-maintained compatibility type (NOT mgen-generated): `uni-syscall.wit`
// carries relation deltas as inline `tuple<u64, u8, list<u8>>` elements, so
// there is no WIT type for mgen to emit. On the wire a delta is the
// positional `[attr, op, datum]` triple of the `relation-update` request
// body; `MuduSysCallApi` converts deltas to that tuple form before encoding
// (byte-identical to the historical `[MessagePackObject]` serialization).
namespace mududb.types {

// relation-level syscall payload element types

/// `relation-update` delta operand sign: `col = col + datum`.
public static class UniRelationDeltaOp {

    // `col = col + datum`
    public const byte Add = 0;

    // `col = col - datum`
    public const byte Sub = 1;

    // Deferred `col = col + datum`, evaluated atomically against the latest
    // committed row at COMMIT APPLY time instead of under the statement lock.
    public const byte AddDeferred = 2;

    // Deferred `col = col - datum`; same apply-time/lock-free contract.
    public const byte SubDeferred = 3;

    // Deferred conditional restock:
    // `col = col - q; if col < floor { col + wrap }`; the datum packs three
    // big-endian i64s `[q, floor, wrap]`.
    public const byte SubWrapDeferred = 4;

}

/// A `SET col = col <+|-> datum` assignment of a `relation-update` call.
public struct UniRelationDelta {

    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniRelationDelta()
    {

        Attr = 0;

        Op = 0;

        Datum = [];

    }


    // target column index in the original table definition

    public ulong Attr { get; set; }


    // `UniRelationDeltaOp.Add` .. `UniRelationDeltaOp.SubWrapDeferred`

    public byte Op { get; set; }


    // operand in the column's binary encoding (MessagePack bin on the wire)

    public required byte[] Datum { get; set; }


    public static UniRelationDelta Add(ulong attr, byte[] datum)
    {
        return new UniRelationDelta { Attr = attr, Op = UniRelationDeltaOp.Add, Datum = datum };
    }

    public static UniRelationDelta Sub(ulong attr, byte[] datum)
    {
        return new UniRelationDelta { Attr = attr, Op = UniRelationDeltaOp.Sub, Datum = datum };
    }

    public static UniRelationDelta AddDeferred(ulong attr, byte[] datum)
    {
        return new UniRelationDelta { Attr = attr, Op = UniRelationDeltaOp.AddDeferred, Datum = datum };
    }

    public static UniRelationDelta SubDeferred(ulong attr, byte[] datum)
    {
        return new UniRelationDelta { Attr = attr, Op = UniRelationDeltaOp.SubDeferred, Datum = datum };
    }

    // Deferred conditional restock; the datum packs three big-endian i64s
    // `[quantity, floor, wrap]`, mirroring `UniRelationDelta::sub_wrap`.
    public static UniRelationDelta SubWrap(ulong attr, long quantity, long floor, long wrap)
    {
        var datum = new byte[24];
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(datum.AsSpan(0, 8), quantity);
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(datum.AsSpan(8, 8), floor);
        global::System.Buffers.Binary.BinaryPrimitives.WriteInt64BigEndian(datum.AsSpan(16, 8), wrap);
        return new UniRelationDelta { Attr = attr, Op = UniRelationDeltaOp.SubWrapDeferred, Datum = datum };
    }

}

}

