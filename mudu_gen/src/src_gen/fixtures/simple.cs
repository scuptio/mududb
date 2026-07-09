namespace MuduDb {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackFormatter(typeof(MuStatusFormatter))]
public enum MuStatus {
    
    
    Ok = 0,
    
    
    Err = 1,
    
}

public class MuStatusFormatter : IMessagePackFormatter<MuStatus>
{
    public void Serialize(ref MessagePackWriter writer, MuStatus value, MessagePackSerializerOptions options)
    {
        writer.Write((uint)value);
    }

    public MuStatus Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        return (MuStatus)reader.ReadUInt32();
    }
}




[MessagePackFormatter(typeof(MuValueFormatter))]
public interface MuValue
{
    public MuValueKind Kind();
}

public enum MuValueKind {

   Integer = 0,

   Text = 1,

}



public class MuValueInteger : MuValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public MuValueInteger()
    {
        Inner = 0;
    }
    

    public long Inner  { get; set; }

    public MuValueKind Kind() {
        return MuValueKind.Integer;
    }

    public static MuValueKind KindStatic() {
        return MuValueKind.Integer;
    }

    public static MuValueInteger AsInteger(MuValue value)
    {
        switch (value)
        {
            case MuValueInteger  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}

public class MuValueText : MuValue
{
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public MuValueText()
    {
        Inner = string.Empty;
    }
    

    public required string Inner  { get; set; }

    public MuValueKind Kind() {
        return MuValueKind.Text;
    }

    public static MuValueKind KindStatic() {
        return MuValueKind.Text;
    }

    public static MuValueText AsText(MuValue value)
    {
        switch (value)
        {
            case MuValueText  v:
                return v;
            default:
                throw new global::System.InvalidOperationException($"Unknown type: {value?.GetType()}");
        }
    }
}


public class MuValueFormatter : IMessagePackFormatter<MuValue?>
{
    public void Serialize(ref MessagePackWriter writer, MuValue? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        writer.WriteArrayHeader(2);
        writer.Write((uint)value.Kind());
        switch (value.Kind())
        {
            
            case MuValueKind.Integer:
                
                MessagePackSerializer.Serialize(ref writer, ((MuValueInteger)value).Inner, options);
                
                break;
            
            case MuValueKind.Text:
                
                MessagePackSerializer.Serialize(ref writer, ((MuValueText)value).Inner, options);
                
                break;
            
            default:
                throw new global::System.InvalidOperationException($"Unknown kind: {value.Kind()}");
        }
    }

    public MuValue? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        var count = reader.ReadArrayHeader();
        if (count != 2)
        {
            throw new global::System.InvalidOperationException($"Expected array of length 2, got {count}");
        }

        var tag = reader.ReadUInt32();
        switch (tag)
        {
            
            case 0:
                
                var integerInner = MessagePackSerializer.Deserialize<long>(ref reader, options);
                return new MuValueInteger { Inner = integerInner };
                
            
            case 1:
                
                var textInner = MessagePackSerializer.Deserialize<string>(ref reader, options)!;
                return new MuValueText { Inner = textInner };
                
            
            default:
                throw new global::System.InvalidOperationException($"Unknown tag: {tag}");
        }
    }
}


[MessagePackFormatter(typeof(MuOidFormatter))]
public struct MuOid {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public MuOid()
    {
        
        H = 0;
        
        L = 0;
        
    }
    
    
    
    public ulong H { get; set; }
    
    
    public ulong L { get; set; }
    
}

public class MuOidFormatter : IMessagePackFormatter<MuOid?>
{
    public void Serialize(ref MessagePackWriter writer, MuOid? value, MessagePackSerializerOptions options)
    {
        if (value is null)
        {
            writer.WriteNil();
            return;
        }

        writer.WriteArrayHeader(2);
        
        MessagePackSerializer.Serialize(ref writer, value.H, options);
        
        MessagePackSerializer.Serialize(ref writer, value.L, options);
        
    }

    public MuOid? Deserialize(ref MessagePackReader reader, MessagePackSerializerOptions options)
    {
        if (reader.TryReadNil())
        {
            return null;
        }

        var count = reader.ReadArrayHeader();
        if (count != 2)
        {
            throw new global::System.InvalidOperationException($"Expected array of length 2, got {count}");
        }

        var value = new MuOid();
        
        value.H = MessagePackSerializer.Deserialize<ulong>(ref reader, options);
        
        value.L = MessagePackSerializer.Deserialize<ulong>(ref reader, options);
        
        return value;
    }
}

}