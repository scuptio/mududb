namespace Universal {

using MessagePack;
using MessagePack.Formatters;
using System.Collections.Generic;




[MessagePackObject]
public struct UniRecordField {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniRecordField()
    {
        
        FieldName = string.Empty;
        
        FieldType = new UniDatTypeIdentifier();
        
    }
    
    
    
    [Key(0)]
    public required string FieldName { get; set; }
    
    
    [Key(1)]
    public required UniDatType FieldType { get; set; }
    
}


[MessagePackObject]
public struct UniRecordType {
    
    [global::System.Diagnostics.CodeAnalysis.SetsRequiredMembers]
    public UniRecordType()
    {
        
        RecordName = string.Empty;
        
        RecordFields = [];
        
    }
    
    
    
    [Key(0)]
    public required string RecordName { get; set; }
    
    
    [Key(1)]
    public required List<UniRecordField> RecordFields { get; set; }
    
}

}
