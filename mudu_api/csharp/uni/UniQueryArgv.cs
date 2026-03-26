namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniQueryArgv {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniQueryArgv()
    {
        
        Oid = new UniOid();
        
        Query = new UniSqlStmt();
        
        ParamList = new UniSqlParam();
        
    }
    
    
    
    [Key(0)]
    public required UniOid Oid { get; set; }
    
    
    [Key(1)]
    public required UniSqlStmt Query { get; set; }
    
    
    [Key(2)]
    public required UniSqlParam ParamList { get; set; }
    
}

}