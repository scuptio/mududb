// Derived from `mgen message -l csharp` on `mudu_binding/wit/uni-fs-stat.wit`;
// hand-normalized to the committed [MessagePackObject] + [Key(n)] model style.
namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniFsStat {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniFsStat()
    {
        
        Oid = new UniOid();
        
        Generation = 0;
        
        Entry = string.Empty;
        
        Length = 0;
        
        State = 0;
        
    }
    
    
    
    [Key(0)]
    public required UniOid Oid { get; set; }
    
    
    [Key(1)]
    public ulong Generation { get; set; }
    
    
    [Key(2)]
    public required string Entry { get; set; }
    
    
    [Key(3)]
    public ulong Length { get; set; }
    
    
    [Key(4)]
    public uint State { get; set; }
    
}

}
