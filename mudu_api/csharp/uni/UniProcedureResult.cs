namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniProcedureResult {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniProcedureResult()
    {
        
        ReturnList = [];
        
    }
    
    
    
    [Key(0)]
    public required List<UniDatValue> ReturnList { get; set; }
    
}

}