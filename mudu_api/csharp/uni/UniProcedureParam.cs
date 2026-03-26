namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniProcedureParam {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniProcedureParam()
    {
        
        Procedure = 0;
        
        Session = new UniOid();
        
        ParamList = [];
        
    }
    
    
    
    [Key(0)]
    public ulong Procedure { get; set; }
    
    
    [Key(1)]
    public required UniOid Session { get; set; }
    
    
    [Key(2)]
    public required List<UniDatValue> ParamList { get; set; }
    
}

}