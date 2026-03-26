namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniDatTypePrimitive))]

[Union(1, typeof(UniDatTypeArray))]

[Union(2, typeof(UniDatTypeRecord))]

[Union(3, typeof(UniDatTypeOption))]

[Union(4, typeof(UniDatTypeTuple))]

[Union(5, typeof(UniDatTypeResult))]

[Union(6, typeof(UniDatTypeBox))]

[Union(7, typeof(UniDatTypeIdentifier))]

public interface UniDatType
{
    public UniDatTypeKind Kind();
}

public enum UniDatTypeKind {

   Primitive = 0,

   Array = 1,

   Record = 2,

   Option = 3,

   Tuple = 4,

   Result = 5,

   Box = 6,

   Identifier = 7,

}



[MessagePackFormatter(typeof(UniDatTypePrimitiveFormatter))]
public class UniDatTypePrimitive : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypePrimitive()
    {
        Inner = default;
    }
    

    public required UniPrimitive Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Primitive;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Primitive;
    }

    public static UniDatTypePrimitive AsPrimitive(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypePrimitive  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypePrimitiveFormatter : IMessagePackFormatter<UniDatTypePrimitive?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypePrimitive? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypePrimitive? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniPrimitive inner = MessagePackSerializer.Deserialize<UniPrimitive>(ref reader, options)!;
        return new UniDatTypePrimitive { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeArrayFormatter))]
public class UniDatTypeArray : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeArray()
    {
        Inner = new UniDatTypeIdentifier();
    }
    

    public required UniDatType Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Array;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Array;
    }

    public static UniDatTypeArray AsArray(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeArray  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeArrayFormatter : IMessagePackFormatter<UniDatTypeArray?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeArray? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeArray? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniDatType inner = MessagePackSerializer.Deserialize<UniDatType>(ref reader, options)!;
        return new UniDatTypeArray { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeRecordFormatter))]
public class UniDatTypeRecord : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeRecord()
    {
        Inner = new UniRecordType();
    }
    

    public required UniRecordType Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Record;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Record;
    }

    public static UniDatTypeRecord AsRecord(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeRecord  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeRecordFormatter : IMessagePackFormatter<UniDatTypeRecord?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeRecord? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeRecord? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniRecordType inner = MessagePackSerializer.Deserialize<UniRecordType>(ref reader, options)!;
        return new UniDatTypeRecord { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeOptionFormatter))]
public class UniDatTypeOption : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeOption()
    {
        Inner = new UniDatTypeIdentifier();
    }
    

    public required UniDatType Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Option;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Option;
    }

    public static UniDatTypeOption AsOption(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeOption  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeOptionFormatter : IMessagePackFormatter<UniDatTypeOption?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeOption? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeOption? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniDatType inner = MessagePackSerializer.Deserialize<UniDatType>(ref reader, options)!;
        return new UniDatTypeOption { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeTupleFormatter))]
public class UniDatTypeTuple : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeTuple()
    {
        Inner = [];
    }
    

    public required List<UniDatType> Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Tuple;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Tuple;
    }

    public static UniDatTypeTuple AsTuple(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeTuple  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeTupleFormatter : IMessagePackFormatter<UniDatTypeTuple?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeTuple? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeTuple? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        List<UniDatType> inner = MessagePackSerializer.Deserialize<List<UniDatType>>(ref reader, options)!;
        return new UniDatTypeTuple { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeResultFormatter))]
public class UniDatTypeResult : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeResult()
    {
        Inner = new UniResultType();
    }
    

    public required UniResultType Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Result;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Result;
    }

    public static UniDatTypeResult AsResult(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeResult  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeResultFormatter : IMessagePackFormatter<UniDatTypeResult?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeResult? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeResult? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniResultType inner = MessagePackSerializer.Deserialize<UniResultType>(ref reader, options)!;
        return new UniDatTypeResult { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeBoxFormatter))]
public class UniDatTypeBox : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeBox()
    {
        Inner = new UniDatTypeIdentifier();
    }
    

    public required UniDatType Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Box;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Box;
    }

    public static UniDatTypeBox AsBox(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeBox  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeBoxFormatter : IMessagePackFormatter<UniDatTypeBox?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeBox? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeBox? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniDatType inner = MessagePackSerializer.Deserialize<UniDatType>(ref reader, options)!;
        return new UniDatTypeBox { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatTypeIdentifierFormatter))]
public class UniDatTypeIdentifier : UniDatType
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatTypeIdentifier()
    {
        Inner = string.Empty;
    }
    

    public required string Inner  { get; set; }

    public UniDatTypeKind Kind() {
        return UniDatTypeKind.Identifier;
    }

    public static UniDatTypeKind KindStatic() {
        return UniDatTypeKind.Identifier;
    }

    public static UniDatTypeIdentifier AsIdentifier(UniDatType value)
    {
        switch (value)
        {
            case UniDatTypeIdentifier  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatTypeIdentifierFormatter : IMessagePackFormatter<UniDatTypeIdentifier?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatTypeIdentifier? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatTypeIdentifier? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        string inner = MessagePackSerializer.Deserialize<string>(ref reader, options)!;
        return new UniDatTypeIdentifier { Inner= inner};
    }
}


}
