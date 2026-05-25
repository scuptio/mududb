namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniScalarValueBool))]

[Union(1, typeof(UniScalarValueU8))]

[Union(2, typeof(UniScalarValueI8))]

[Union(3, typeof(UniScalarValueU16))]

[Union(4, typeof(UniScalarValueI16))]

[Union(5, typeof(UniScalarValueU32))]

[Union(6, typeof(UniScalarValueI32))]

[Union(7, typeof(UniScalarValueU64))]

[Union(8, typeof(UniScalarValueI64))]

[Union(9, typeof(UniScalarValueF32))]

[Union(10, typeof(UniScalarValueF64))]

[Union(11, typeof(UniScalarValueChar))]

[Union(12, typeof(UniScalarValueString))]

[Union(13, typeof(UniScalarValueBlob))]

public interface UniScalarValue
{
    public UniScalarValueKind Kind();
}

public enum UniScalarValueKind {

   Bool = 0,

   U8 = 1,

   I8 = 2,

   U16 = 3,

   I16 = 4,

   U32 = 5,

   I32 = 6,

   U64 = 7,

   I64 = 8,

   F32 = 9,

   F64 = 10,

   Char = 11,

   String = 12,

   Blob = 13,

}



[MessagePackFormatter(typeof(UniScalarValueBoolFormatter))]
public class UniScalarValueBool : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueBool()
    {
        Inner = false;
    }
    

    public bool Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.Bool;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.Bool;
    }

    public static UniScalarValueBool AsBool(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueBool  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueBoolFormatter : IMessagePackFormatter<UniScalarValueBool?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueBool? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueBool? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        bool inner = MessagePackSerializer.Deserialize<bool>(ref reader, options);
        return new UniScalarValueBool { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueU8Formatter))]
public class UniScalarValueU8 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueU8()
    {
        Inner = 0;
    }
    

    public byte Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.U8;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.U8;
    }

    public static UniScalarValueU8 AsU8(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueU8  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueU8Formatter : IMessagePackFormatter<UniScalarValueU8?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueU8? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueU8? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte inner = MessagePackSerializer.Deserialize<byte>(ref reader, options);
        return new UniScalarValueU8 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueI8Formatter))]
public class UniScalarValueI8 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueI8()
    {
        Inner = 0;
    }
    

    public byte Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.I8;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.I8;
    }

    public static UniScalarValueI8 AsI8(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueI8  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueI8Formatter : IMessagePackFormatter<UniScalarValueI8?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueI8? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueI8? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte inner = MessagePackSerializer.Deserialize<byte>(ref reader, options);
        return new UniScalarValueI8 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueU16Formatter))]
public class UniScalarValueU16 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueU16()
    {
        Inner = 0;
    }
    

    public ushort Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.U16;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.U16;
    }

    public static UniScalarValueU16 AsU16(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueU16  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueU16Formatter : IMessagePackFormatter<UniScalarValueU16?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueU16? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueU16? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        ushort inner = MessagePackSerializer.Deserialize<ushort>(ref reader, options);
        return new UniScalarValueU16 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueI16Formatter))]
public class UniScalarValueI16 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueI16()
    {
        Inner = 0;
    }
    

    public short Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.I16;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.I16;
    }

    public static UniScalarValueI16 AsI16(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueI16  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueI16Formatter : IMessagePackFormatter<UniScalarValueI16?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueI16? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueI16? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        short inner = MessagePackSerializer.Deserialize<short>(ref reader, options);
        return new UniScalarValueI16 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueU32Formatter))]
public class UniScalarValueU32 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueU32()
    {
        Inner = 0;
    }
    

    public uint Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.U32;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.U32;
    }

    public static UniScalarValueU32 AsU32(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueU32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueU32Formatter : IMessagePackFormatter<UniScalarValueU32?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueU32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueU32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        uint inner = MessagePackSerializer.Deserialize<uint>(ref reader, options);
        return new UniScalarValueU32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueI32Formatter))]
public class UniScalarValueI32 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueI32()
    {
        Inner = 0;
    }
    

    public int Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.I32;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.I32;
    }

    public static UniScalarValueI32 AsI32(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueI32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueI32Formatter : IMessagePackFormatter<UniScalarValueI32?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueI32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueI32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        int inner = MessagePackSerializer.Deserialize<int>(ref reader, options);
        return new UniScalarValueI32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueU64Formatter))]
public class UniScalarValueU64 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueU64()
    {
        Inner = 0;
    }
    

    public ulong Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.U64;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.U64;
    }

    public static UniScalarValueU64 AsU64(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueU64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueU64Formatter : IMessagePackFormatter<UniScalarValueU64?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueU64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueU64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        ulong inner = MessagePackSerializer.Deserialize<ulong>(ref reader, options);
        return new UniScalarValueU64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueI64Formatter))]
public class UniScalarValueI64 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueI64()
    {
        Inner = 0;
    }
    

    public long Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.I64;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.I64;
    }

    public static UniScalarValueI64 AsI64(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueI64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueI64Formatter : IMessagePackFormatter<UniScalarValueI64?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueI64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueI64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        long inner = MessagePackSerializer.Deserialize<long>(ref reader, options);
        return new UniScalarValueI64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueF32Formatter))]
public class UniScalarValueF32 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueF32()
    {
        Inner = 0;
    }
    

    public float Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.F32;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.F32;
    }

    public static UniScalarValueF32 AsF32(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueF32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueF32Formatter : IMessagePackFormatter<UniScalarValueF32?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueF32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueF32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        float inner = MessagePackSerializer.Deserialize<float>(ref reader, options);
        return new UniScalarValueF32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueF64Formatter))]
public class UniScalarValueF64 : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueF64()
    {
        Inner = 0;
    }
    

    public double Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.F64;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.F64;
    }

    public static UniScalarValueF64 AsF64(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueF64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueF64Formatter : IMessagePackFormatter<UniScalarValueF64?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueF64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueF64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        double inner = MessagePackSerializer.Deserialize<double>(ref reader, options);
        return new UniScalarValueF64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueCharFormatter))]
public class UniScalarValueChar : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueChar()
    {
        Inner = '\0';
    }
    

    public char Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.Char;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.Char;
    }

    public static UniScalarValueChar AsChar(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueChar  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueCharFormatter : IMessagePackFormatter<UniScalarValueChar?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueChar? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueChar? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        char inner = MessagePackSerializer.Deserialize<char>(ref reader, options);
        return new UniScalarValueChar { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueStringFormatter))]
public class UniScalarValueString : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueString()
    {
        Inner = string.Empty;
    }
    

    public required string Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.String;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.String;
    }

    public static UniScalarValueString AsString(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueString  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueStringFormatter : IMessagePackFormatter<UniScalarValueString?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueString? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueString? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        string inner = MessagePackSerializer.Deserialize<string>(ref reader, options)!;
        return new UniScalarValueString { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniScalarValueBlobFormatter))]
public class UniScalarValueBlob : UniScalarValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniScalarValueBlob()
    {
        Inner = [];
    }
    

    public required byte[] Inner  { get; set; }

    public UniScalarValueKind Kind() {
        return UniScalarValueKind.Blob;
    }

    public static UniScalarValueKind KindStatic() {
        return UniScalarValueKind.Blob;
    }

    public static UniScalarValueBlob AsBlob(UniScalarValue value)
    {
        switch (value)
        {
            case UniScalarValueBlob  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniScalarValueBlobFormatter : IMessagePackFormatter<UniScalarValueBlob?>
{
    public void Serialize(ref MessagePackWriter writer, UniScalarValueBlob? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniScalarValueBlob? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte[] inner = MessagePackSerializer.Deserialize<byte[]>(ref reader, options)!;
        return new UniScalarValueBlob { Inner= inner};
    }
}


}