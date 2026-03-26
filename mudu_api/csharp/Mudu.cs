#nullable enable

global using UniCommandArgv = Universal.UniCommandArgv;
global using UniCommandResult = Universal.UniCommandResult;
global using UniCommandReturn = Universal.UniCommandReturn;
global using UniCommandReturnErr = Universal.UniCommandReturnErr;
global using UniCommandReturnKind = Universal.UniCommandReturnKind;
global using UniCommandReturnOk = Universal.UniCommandReturnOk;
global using UniDatType = Universal.UniDatType;
global using UniDatValue = Universal.UniDatValue;
global using UniError = Universal.UniError;
global using UniMessage = Universal.UniMessage;
global using UniOid = Universal.UniOid;
global using UniPrimitive = Universal.UniPrimitive;
global using UniPrimitiveValue = Universal.UniPrimitiveValue;
global using UniQueryArgv = Universal.UniQueryArgv;
global using UniQueryResult = Universal.UniQueryResult;
global using UniQueryReturn = Universal.UniQueryReturn;
global using UniQueryReturnErr = Universal.UniQueryReturnErr;
global using UniQueryReturnKind = Universal.UniQueryReturnKind;
global using UniQueryReturnOk = Universal.UniQueryReturnOk;
global using UniRecordField = Universal.UniRecordField;
global using UniRecordType = Universal.UniRecordType;
global using UniResultSet = Universal.UniResultSet;
global using UniSqlParam = Universal.UniSqlParam;
global using UniSqlStmt = Universal.UniSqlStmt;
global using UniTupleRow = Universal.UniTupleRow;

using MessagePack;

namespace Mudu.Api;

public static class Mudu
{
    public static CommandResponse Command(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return new CommandResponse(MuduSysCallApi.SysCommand(argv, options));
    }

    public static QueryResponse Query(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        return new QueryResponse(MuduSysCallApi.SysQuery(argv, options));
    }

    public static byte[] Serialize(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return MuduSysCallApi.SerializeCommand(argv, options);
    }

    public static byte[] Serialize(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        return MuduSysCallApi.SerializeQuery(argv, options);
    }

    public static UniCommandReturn DeserializeCommand(byte[] bytes, MessagePackSerializerOptions? options = null)
    {
        return MuduSysCallApi.DeserializeCommandResult(bytes, options);
    }

    public static UniQueryReturn DeserializeQuery(byte[] bytes, MessagePackSerializerOptions? options = null)
    {
        return MuduSysCallApi.DeserializeQueryResult(bytes, options);
    }
}

public readonly struct CommandResponse
{
    private readonly UniCommandReturn inner;

    public CommandResponse(UniCommandReturn inner)
    {
        this.inner = inner;
    }

    public UniCommandReturn Raw => inner;

    public bool IsOk => inner.Kind() == UniCommandReturnKind.Ok;

    public bool IsErr => inner.Kind() == UniCommandReturnKind.Err;

    public UniCommandResult? Result => IsOk ? UniCommandReturnOk.AsOk(inner).Inner : null;

    public UniError? Error => IsErr ? UniCommandReturnErr.AsErr(inner).Inner : null;

    public ulong? AffectedRows => Result?.AffectedRows;

    public UniCommandResult RequireOk()
    {
        if (IsOk)
        {
            return UniCommandReturnOk.AsOk(inner).Inner;
        }

        throw new global::System.InvalidOperationException(UniCommandReturnErr.AsErr(inner).Inner.ErrMsg);
    }
}

public readonly struct QueryResponse
{
    private readonly UniQueryReturn inner;

    public QueryResponse(UniQueryReturn inner)
    {
        this.inner = inner;
    }

    public UniQueryReturn Raw => inner;

    public bool IsOk => inner.Kind() == UniQueryReturnKind.Ok;

    public bool IsErr => inner.Kind() == UniQueryReturnKind.Err;

    public UniQueryResult? Result => IsOk ? UniQueryReturnOk.AsOk(inner).Inner : null;

    public UniError? Error => IsErr ? UniQueryReturnErr.AsErr(inner).Inner : null;

    public UniRecordType? TupleDesc => Result?.TupleDesc;

    public UniResultSet? ResultSet => Result?.ResultSet;

    public UniQueryResult RequireOk()
    {
        if (IsOk)
        {
            return UniQueryReturnOk.AsOk(inner).Inner;
        }

        throw new global::System.InvalidOperationException(UniQueryReturnErr.AsErr(inner).Inner.ErrMsg);
    }
}
