namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniDatValueScalar))]

[Union(1, typeof(UniDatValueArray))]

[Union(2, typeof(UniDatValueRecord))]

[Union(3, typeof(UniDatValueBinary))]

public interface UniDatValue
{
    public UniDatValueKind Kind();
}

public enum UniDatValueKind {

   Scalar = 0,

   Array = 1,

   Record = 2,

   Binary = 3,

}



[MessagePackFormatter(typeof(UniDatValueScalarFormatter))]
public class UniDatValueScalar : UniDatValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatValueScalar()
    {
        Inner = new UniScalarValueString();
    }
    

    public required UniScalarValue Inner  { get; set; }

    public UniDatValueKind Kind() {
        return UniDatValueKind.Scalar;
    }

    public static UniDatValueKind KindStatic() {
        return UniDatValueKind.Scalar;
    }

    public static UniDatValueScalar AsScalar(UniDatValue value)
    {
        switch (value)
        {
            case UniDatValueScalar  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatValueScalarFormatter : IMessagePackFormatter<UniDatValueScalar?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatValueScalar? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatValueScalar? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniScalarValue inner = MessagePackSerializer.Deserialize<UniScalarValue>(ref reader, options)!;
        return new UniDatValueScalar { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatValueArrayFormatter))]
public class UniDatValueArray : UniDatValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatValueArray()
    {
        Inner = [];
    }
    

    public required List<UniDatValue> Inner  { get; set; }

    public UniDatValueKind Kind() {
        return UniDatValueKind.Array;
    }

    public static UniDatValueKind KindStatic() {
        return UniDatValueKind.Array;
    }

    public static UniDatValueArray AsArray(UniDatValue value)
    {
        switch (value)
        {
            case UniDatValueArray  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatValueArrayFormatter : IMessagePackFormatter<UniDatValueArray?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatValueArray? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatValueArray? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        List<UniDatValue> inner = MessagePackSerializer.Deserialize<List<UniDatValue>>(ref reader, options)!;
        return new UniDatValueArray { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatValueRecordFormatter))]
public class UniDatValueRecord : UniDatValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatValueRecord()
    {
        Inner = [];
    }
    

    public required List<UniDatValue> Inner  { get; set; }

    public UniDatValueKind Kind() {
        return UniDatValueKind.Record;
    }

    public static UniDatValueKind KindStatic() {
        return UniDatValueKind.Record;
    }

    public static UniDatValueRecord AsRecord(UniDatValue value)
    {
        switch (value)
        {
            case UniDatValueRecord  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatValueRecordFormatter : IMessagePackFormatter<UniDatValueRecord?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatValueRecord? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatValueRecord? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        List<UniDatValue> inner = MessagePackSerializer.Deserialize<List<UniDatValue>>(ref reader, options)!;
        return new UniDatValueRecord { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniDatValueBinaryFormatter))]
public class UniDatValueBinary : UniDatValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniDatValueBinary()
    {
        Inner = [];
    }
    

    public required byte[] Inner  { get; set; }

    public UniDatValueKind Kind() {
        return UniDatValueKind.Binary;
    }

    public static UniDatValueKind KindStatic() {
        return UniDatValueKind.Binary;
    }

    public static UniDatValueBinary AsBinary(UniDatValue value)
    {
        switch (value)
        {
            case UniDatValueBinary  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniDatValueBinaryFormatter : IMessagePackFormatter<UniDatValueBinary?>
{
    public void Serialize(ref MessagePackWriter writer, UniDatValueBinary? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniDatValueBinary? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        byte[] inner = MessagePackSerializer.Deserialize<byte[]>(ref reader, options)!;
        return new UniDatValueBinary { Inner= inner};
    }
}


}
