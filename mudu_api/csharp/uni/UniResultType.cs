namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniResultType {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniResultType()
    {
        
        Ok = new UniDatTypeIdentifier();
        
        Err = new UniDatTypeIdentifier();
        
    }
    
    
    
    [Key(0)]
    public required UniDatType Ok { get; set; }
    
    
    [Key(1)]
    public required UniDatType Err { get; set; }
    
}

}
