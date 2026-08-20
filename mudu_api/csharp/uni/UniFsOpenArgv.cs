// Derived from `mgen message -l csharp` on `mudu_binding/wit/uni-fs-open-argv.wit`;
// hand-normalized to the committed [MessagePackObject] + [Key(n)] model style.
namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniFsOpenArgv {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniFsOpenArgv()
    {
        
        Session = new UniOid();
        
        Oid = new UniOid();
        
        Path = string.Empty;
        
        Flags = 0;
        
    }
    
    
    
    [Key(0)]
    public required UniOid Session { get; set; }
    
    
    [Key(1)]
    public required UniOid Oid { get; set; }
    
    
    [Key(2)]
    public required string Path { get; set; }
    
    
    [Key(3)]
    public uint Flags { get; set; }
    
}

}
