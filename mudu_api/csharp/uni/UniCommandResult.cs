namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;





// Annotate inheritance types

[Union(0, typeof(UniCommandReturnOk))]

[Union(1, typeof(UniCommandReturnErr))]

public interface UniCommandReturn
{
    public UniCommandReturnKind Kind();
}

public enum UniCommandReturnKind {

   Ok = 0,

   Err = 1,

}



[MessagePackFormatter(typeof(UniCommandReturnOkFormatter))]
public class UniCommandReturnOk : UniCommandReturn
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniCommandReturnOk()
    {
        Inner = new UniCommandResult();
    }
    

    public required UniCommandResult Inner  { get; set; }

    public UniCommandReturnKind Kind() {
        return UniCommandReturnKind.Ok;
    }

    public static UniCommandReturnKind KindStatic() {
        return UniCommandReturnKind.Ok;
    }

    public static UniCommandReturnOk AsOk(UniCommandReturn value)
    {
        switch (value)
        {
            case UniCommandReturnOk  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniCommandReturnOkFormatter : IMessagePackFormatter<UniCommandReturnOk?>
{
    public void Serialize(ref MessagePackWriter writer, UniCommandReturnOk? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniCommandReturnOk? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniCommandResult inner = MessagePackSerializer.Deserialize<UniCommandResult>(ref reader, options)!;
        return new UniCommandReturnOk { Inner= inner};
    }
}

[MessagePackFormatter(typeof(UniCommandReturnErrFormatter))]
public class UniCommandReturnErr : UniCommandReturn
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniCommandReturnErr()
    {
        Inner = new UniError();
    }
    

    public required UniError Inner  { get; set; }

    public UniCommandReturnKind Kind() {
        return UniCommandReturnKind.Err;
    }

    public static UniCommandReturnKind KindStatic() {
        return UniCommandReturnKind.Err;
    }

    public static UniCommandReturnErr AsErr(UniCommandReturn value)
    {
        switch (value)
        {
            case UniCommandReturnErr  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class UniCommandReturnErrFormatter : IMessagePackFormatter<UniCommandReturnErr?>
{
    public void Serialize(ref MessagePackWriter writer, UniCommandReturnErr? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        MessagePackSerializer.Serialize(ref writer, value.Inner, options);
    }

    public UniCommandReturnErr? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        UniError inner = MessagePackSerializer.Deserialize<UniError>(ref reader, options)!;
        return new UniCommandReturnErr { Inner= inner};
    }
}



[MessagePackObject]
public struct UniCommandResult {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniCommandResult()
    {
        
        AffectedRows = 0;
        
    }
    
    
    
    [Key(0)]
    public ulong AffectedRows { get; set; }
    
}

}