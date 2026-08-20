namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniResultSet {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniResultSet()
    {
        
        Eof = false;
        
        RowSet = [];
        
        Cursor = [];
        
    }
    
    
    
    [Key(0)]
    public bool Eof { get; set; }
    
    
    [Key(1)]
    public required List<UniTupleRow> RowSet { get; set; }
    
    
    // Encoded as a MessagePack array of u8 (not a bin) to match the Rust
    // host's derived serde for `cursor: Vec<u8>`; see UniError.ErrDetails.
    [Key(2)]
    public required List<byte> Cursor { get; set; }
    
}

}