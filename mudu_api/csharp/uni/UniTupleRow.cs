namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniTupleRow {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniTupleRow()
    {
        
        Fields = [];
        
    }
    
    
    
    [Key(0)]
    public required List<UniDatValue> Fields { get; set; }
    
}

}