namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniQueryReturnOk))]

[Union(1, typeof(UniQueryReturnErr))]

public interface UniQueryReturn
{
    public UniQueryReturnKind Kind();
}

public enum UniQueryReturnKind {

   Ok = 0,

   Err = 1,

}



[MessagePackFormatter(typeof(UniQueryReturnOkFormatter))]
public class UniQueryReturnOk : UniQueryReturn
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniQueryReturnOk()
    {
        Inner = new UniQueryResult();
    }
    

    public required UniQueryResult Inner  { get; set; }

    public UniQueryReturnKind Kind() {
        return UniQueryReturnKind.Ok;
    }

    public static UniQueryReturnKind KindStatic() {
        return UniQueryReturnKind.Ok;
    }

    public static UniQueryReturnOk AsOk(UniQueryReturn value)
    {
        switch (value)
        {
            case UniQueryReturnOk  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniQueryReturnOkFormatter : IMessagePackFormatter<UniQueryReturnOk?>
{
    public void Serialize(ref MessagePackWriter writer, UniQueryReturnOk? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniQueryReturnOk? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniQueryResult inner = MessagePackSerializer.Deserialize<UniQueryResult>(ref reader, options)!;
        return new UniQueryReturnOk { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniQueryReturnErrFormatter))]
public class UniQueryReturnErr : UniQueryReturn
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniQueryReturnErr()
    {
        Inner = new UniError();
    }
    

    public required UniError Inner  { get; set; }

    public UniQueryReturnKind Kind() {
        return UniQueryReturnKind.Err;
    }

    public static UniQueryReturnKind KindStatic() {
        return UniQueryReturnKind.Err;
    }

    public static UniQueryReturnErr AsErr(UniQueryReturn value)
    {
        switch (value)
        {
            case UniQueryReturnErr  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniQueryReturnErrFormatter : IMessagePackFormatter<UniQueryReturnErr?>
{
    public void Serialize(ref MessagePackWriter writer, UniQueryReturnErr? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniQueryReturnErr? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniError inner = MessagePackSerializer.Deserialize<UniError>(ref reader, options)!;
        return new UniQueryReturnErr { Inner= inner};
    }
}



[MessagePackObject]
public struct UniQueryResult {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniQueryResult()
    {
        
        TupleDesc = new UniRecordType();
        
        ResultSet = new UniResultSet();
        
    }
    
    
    
    [Key(0)]
    public required UniRecordType TupleDesc { get; set; }
    
    
    [Key(1)]
    public required UniResultSet ResultSet { get; set; }
    
}

}