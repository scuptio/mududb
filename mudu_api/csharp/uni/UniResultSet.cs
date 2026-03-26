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
    
    
    [Key(2)]
    public required byte[] Cursor { get; set; }
    
}

}