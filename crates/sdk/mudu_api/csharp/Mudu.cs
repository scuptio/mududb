#nullable enable

global using UniCommandArgv = mududb.types.UniCommandArgv;
global using UniCommandResult = mududb.types.UniCommandResult;
global using UniCommandReturn = mududb.types.UniCommandReturn;
global using UniCommandReturnErr = mududb.types.UniCommandReturnErr;
global using UniCommandReturnKind = mududb.types.UniCommandReturnKind;
global using UniCommandReturnOk = mududb.types.UniCommandReturnOk;
global using UniDataType = mududb.types.UniDataType;
global using UniDataValue = mududb.types.UniDataValue;
global using UniError = mududb.types.UniError;
global using UniFsDirent = mududb.types.UniFsDirent;
global using UniFsOpenArgv = mududb.types.UniFsOpenArgv;
global using UniFsStat = mududb.types.UniFsStat;
global using UniMessage = mududb.types.UniMessage;
global using UniOid = mududb.types.UniOid;
global using UniScalar = mududb.types.UniScalar;
global using UniScalarValue = mududb.types.UniScalarValue;
global using UniQueryArgv = mududb.types.UniQueryArgv;
global using UniQueryResult = mududb.types.UniQueryResult;
global using UniQueryReturn = mududb.types.UniQueryReturn;
global using UniQueryReturnErr = mududb.types.UniQueryReturnErr;
global using UniQueryReturnKind = mududb.types.UniQueryReturnKind;
global using UniQueryReturnOk = mududb.types.UniQueryReturnOk;
global using UniRecordField = mududb.types.UniRecordField;
global using UniRecordType = mududb.types.UniRecordType;
global using UniRelationDelta = mududb.types.UniRelationDelta;
global using UniRelationDeltaOp = mududb.types.UniRelationDeltaOp;
global using UniResultSet = mududb.types.UniResultSet;
global using UniSqlParam = mududb.types.UniSqlParam;
global using UniSqlStmt = mududb.types.UniSqlStmt;
global using UniTupleRow = mududb.types.UniTupleRow;

using MessagePack;
using mududb.sys;

namespace mududb;

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
