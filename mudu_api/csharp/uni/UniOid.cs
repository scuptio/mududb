namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




// object id

[MessagePackObject]
public struct UniOid {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniOid()
    {
        
        H = 0;
        
        L = 0;
        
    }
    
    
    
    // higher 64 bits
    
    [Key(0)]
    public ulong H { get; set; }
    
    
    // lower 64 bits
    
    [Key(1)]
    public ulong L { get; set; }
    
}

}