// Derived from `mgen message -l csharp` on `mudu_binding/wit/uni-fs-dirent.wit`;
// hand-normalized to the committed [MessagePackObject] + [Key(n)] model style.
namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniFsDirent {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniFsDirent()
    {
        
        Name = string.Empty;
        
        IsDir = false;
        
        Length = 0;
        
    }
    
    
    
    [Key(0)]
    public required string Name { get; set; }
    
    
    [Key(1)]
    public bool IsDir { get; set; }
    
    
    [Key(2)]
    public ulong Length { get; set; }
    
}

}
