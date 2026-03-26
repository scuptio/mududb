namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniSqlStmt {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniSqlStmt()
    {
        
        SqlString = string.Empty;
        
    }
    
    
    
    [Key(0)]
    public required string SqlString { get; set; }
    
}

}