namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniSqlParam {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniSqlParam()
    {
        
        Params = [];
        
    }
    
    
    
    [Key(0)]
    public required List<UniDatValue> Params { get; set; }
    
}

}