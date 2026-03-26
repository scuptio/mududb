namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniError {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniError()
    {
        
        ErrCode = 0;
        
        ErrMsg = string.Empty;
        
    }
    
    
    
    [Key(0)]
    public uint ErrCode { get; set; }
    
    
    [Key(1)]
    public required string ErrMsg { get; set; }
    
}

}