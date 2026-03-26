namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniPrimitiveValueBool))]

[Union(1, typeof(UniPrimitiveValueU8))]

[Union(2, typeof(UniPrimitiveValueI8))]

[Union(3, typeof(UniPrimitiveValueU16))]

[Union(4, typeof(UniPrimitiveValueI16))]

[Union(5, typeof(UniPrimitiveValueU32))]

[Union(6, typeof(UniPrimitiveValueI32))]

[Union(7, typeof(UniPrimitiveValueU64))]

[Union(8, typeof(UniPrimitiveValueI64))]

[Union(9, typeof(UniPrimitiveValueF32))]

[Union(10, typeof(UniPrimitiveValueF64))]

[Union(11, typeof(UniPrimitiveValueChar))]

[Union(12, typeof(UniPrimitiveValueString))]

[Union(13, typeof(UniPrimitiveValueBlob))]

public interface UniPrimitiveValue
{
    public UniPrimitiveValueKind Kind();
}

public enum UniPrimitiveValueKind {

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



[MessagePackFormatter(typeof(UniPrimitiveValueBoolFormatter))]
public class UniPrimitiveValueBool : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueBool()
    {
        Inner = false;
    }
    

    public bool Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.Bool;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.Bool;
    }

    public static UniPrimitiveValueBool AsBool(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueBool  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueBoolFormatter : IMessagePackFormatter<UniPrimitiveValueBool?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueBool? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueBool? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        bool inner = MessagePackSerializer.Deserialize<bool>(ref reader, options);
        return new UniPrimitiveValueBool { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueU8Formatter))]
public class UniPrimitiveValueU8 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueU8()
    {
        Inner = 0;
    }
    

    public byte Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.U8;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.U8;
    }

    public static UniPrimitiveValueU8 AsU8(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueU8  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueU8Formatter : IMessagePackFormatter<UniPrimitiveValueU8?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueU8? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueU8? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte inner = MessagePackSerializer.Deserialize<byte>(ref reader, options);
        return new UniPrimitiveValueU8 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueI8Formatter))]
public class UniPrimitiveValueI8 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueI8()
    {
        Inner = 0;
    }
    

    public byte Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.I8;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.I8;
    }

    public static UniPrimitiveValueI8 AsI8(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueI8  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueI8Formatter : IMessagePackFormatter<UniPrimitiveValueI8?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueI8? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueI8? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte inner = MessagePackSerializer.Deserialize<byte>(ref reader, options);
        return new UniPrimitiveValueI8 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueU16Formatter))]
public class UniPrimitiveValueU16 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueU16()
    {
        Inner = 0;
    }
    

    public ushort Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.U16;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.U16;
    }

    public static UniPrimitiveValueU16 AsU16(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueU16  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueU16Formatter : IMessagePackFormatter<UniPrimitiveValueU16?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueU16? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueU16? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        ushort inner = MessagePackSerializer.Deserialize<ushort>(ref reader, options);
        return new UniPrimitiveValueU16 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueI16Formatter))]
public class UniPrimitiveValueI16 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueI16()
    {
        Inner = 0;
    }
    

    public short Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.I16;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.I16;
    }

    public static UniPrimitiveValueI16 AsI16(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueI16  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueI16Formatter : IMessagePackFormatter<UniPrimitiveValueI16?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueI16? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueI16? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        short inner = MessagePackSerializer.Deserialize<short>(ref reader, options);
        return new UniPrimitiveValueI16 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueU32Formatter))]
public class UniPrimitiveValueU32 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueU32()
    {
        Inner = 0;
    }
    

    public uint Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.U32;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.U32;
    }

    public static UniPrimitiveValueU32 AsU32(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueU32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueU32Formatter : IMessagePackFormatter<UniPrimitiveValueU32?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueU32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueU32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        uint inner = MessagePackSerializer.Deserialize<uint>(ref reader, options);
        return new UniPrimitiveValueU32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueI32Formatter))]
public class UniPrimitiveValueI32 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueI32()
    {
        Inner = 0;
    }
    

    public int Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.I32;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.I32;
    }

    public static UniPrimitiveValueI32 AsI32(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueI32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueI32Formatter : IMessagePackFormatter<UniPrimitiveValueI32?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueI32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueI32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        int inner = MessagePackSerializer.Deserialize<int>(ref reader, options);
        return new UniPrimitiveValueI32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueU64Formatter))]
public class UniPrimitiveValueU64 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueU64()
    {
        Inner = 0;
    }
    

    public ulong Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.U64;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.U64;
    }

    public static UniPrimitiveValueU64 AsU64(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueU64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueU64Formatter : IMessagePackFormatter<UniPrimitiveValueU64?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueU64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueU64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        ulong inner = MessagePackSerializer.Deserialize<ulong>(ref reader, options);
        return new UniPrimitiveValueU64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueI64Formatter))]
public class UniPrimitiveValueI64 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueI64()
    {
        Inner = 0;
    }
    

    public long Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.I64;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.I64;
    }

    public static UniPrimitiveValueI64 AsI64(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueI64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueI64Formatter : IMessagePackFormatter<UniPrimitiveValueI64?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueI64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueI64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        long inner = MessagePackSerializer.Deserialize<long>(ref reader, options);
        return new UniPrimitiveValueI64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueF32Formatter))]
public class UniPrimitiveValueF32 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueF32()
    {
        Inner = 0;
    }
    

    public float Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.F32;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.F32;
    }

    public static UniPrimitiveValueF32 AsF32(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueF32  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueF32Formatter : IMessagePackFormatter<UniPrimitiveValueF32?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueF32? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueF32? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        float inner = MessagePackSerializer.Deserialize<float>(ref reader, options);
        return new UniPrimitiveValueF32 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueF64Formatter))]
public class UniPrimitiveValueF64 : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueF64()
    {
        Inner = 0;
    }
    

    public double Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.F64;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.F64;
    }

    public static UniPrimitiveValueF64 AsF64(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueF64  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueF64Formatter : IMessagePackFormatter<UniPrimitiveValueF64?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueF64? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueF64? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        double inner = MessagePackSerializer.Deserialize<double>(ref reader, options);
        return new UniPrimitiveValueF64 { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueCharFormatter))]
public class UniPrimitiveValueChar : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueChar()
    {
        Inner = '\0';
    }
    

    public char Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.Char;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.Char;
    }

    public static UniPrimitiveValueChar AsChar(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueChar  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueCharFormatter : IMessagePackFormatter<UniPrimitiveValueChar?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueChar? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueChar? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        char inner = MessagePackSerializer.Deserialize<char>(ref reader, options);
        return new UniPrimitiveValueChar { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueStringFormatter))]
public class UniPrimitiveValueString : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueString()
    {
        Inner = string.Empty;
    }
    

    public required string Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.String;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.String;
    }

    public static UniPrimitiveValueString AsString(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueString  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueStringFormatter : IMessagePackFormatter<UniPrimitiveValueString?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueString? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueString? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        string inner = MessagePackSerializer.Deserialize<string>(ref reader, options)!;
        return new UniPrimitiveValueString { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniPrimitiveValueBlobFormatter))]
public class UniPrimitiveValueBlob : UniPrimitiveValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniPrimitiveValueBlob()
    {
        Inner = [];
    }
    

    public required byte[] Inner  { get; set; }

    public UniPrimitiveValueKind Kind() {
        return UniPrimitiveValueKind.Blob;
    }

    public static UniPrimitiveValueKind KindStatic() {
        return UniPrimitiveValueKind.Blob;
    }

    public static UniPrimitiveValueBlob AsBlob(UniPrimitiveValue value)
    {
        switch (value)
        {
            case UniPrimitiveValueBlob  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniPrimitiveValueBlobFormatter : IMessagePackFormatter<UniPrimitiveValueBlob?>
{
    public void Serialize(ref MessagePackWriter writer, UniPrimitiveValueBlob? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniPrimitiveValueBlob? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte[] inner = MessagePackSerializer.Deserialize<byte[]>(ref reader, options)!;
        return new UniPrimitiveValueBlob { Inner= inner};
    }
}


}