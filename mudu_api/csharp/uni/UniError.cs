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
        
        ErrSrc = string.Empty;
        
        ErrLoc = string.Empty;
        
        ErrDetails = [];
        
    }
    
    
    
    [Key(0)]
    public uint ErrCode { get; set; }
    
    
    [Key(1)]
    public required string ErrMsg { get; set; }
    
    
    [Key(2)]
    public required string ErrSrc { get; set; }
    
    
    [Key(3)]
    public required string ErrLoc { get; set; }
    
    
    // `list<u8>` on the wire is a MessagePack ARRAY of u8 here: the Rust host
    // encodes UniError with a plain serde derive, which maps Vec<u8> to an
    // array (not a bin). List<byte> matches that shape on encode and decode;
    // byte[] would encode as bin and fail to decode the host's array form.
    [Key(4)]
    public required List<byte> ErrDetails { get; set; }
    
}

}
