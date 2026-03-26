namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniCommandArgv {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniCommandArgv()
    {
        
        Oid = new UniOid();
        
        Command = new UniSqlStmt();
        
        ParamList = new UniSqlParam();
        
    }
    
    
    
    [Key(0)]
    public required UniOid Oid { get; set; }
    
    
    [Key(1)]
    public required UniSqlStmt Command { get; set; }
    
    
    [Key(2)]
    public required UniSqlParam ParamList { get; set; }
    
}

}