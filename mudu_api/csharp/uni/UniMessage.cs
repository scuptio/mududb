namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniMessage {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniMessage()
    {
        
        MessageId = 0;
        
        SourceOid = new UniOid();
        
        DestinationOid = new UniOid();
        
        Payload = [];
        
    }
    
    
    
    [Key(0)]
    public uint MessageId { get; set; }
    
    
    [Key(1)]
    public required UniOid SourceOid { get; set; }
    
    
    [Key(2)]
    public required UniOid DestinationOid { get; set; }
    
    
    [Key(3)]
    public required byte[] Payload { get; set; }
    
}

}