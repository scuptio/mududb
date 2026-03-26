#nullable enable

using MessagePack;

namespace Mudu.Api;

public static class MuduSysCallApi
{
    public static byte[] QueryRaw(byte[] queryIn)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.QueryRaw(queryIn);
#else
        return MuduSys.WasmMuduSysCall.QueryRaw(queryIn);
#endif
    }

    public static byte[] QueryRaw(global::System.ReadOnlyMemory<byte> queryIn)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.QueryRaw(queryIn);
#else
        return MuduSys.WasmMuduSysCall.QueryRaw(queryIn);
#endif
    }

    public static byte[] CommandRaw(byte[] commandIn)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.CommandRaw(commandIn);
#else
        return MuduSys.WasmMuduSysCall.CommandRaw(commandIn);
#endif
    }

    public static byte[] CommandRaw(global::System.ReadOnlyMemory<byte> commandIn)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.CommandRaw(commandIn);
#else
        return MuduSys.WasmMuduSysCall.CommandRaw(commandIn);
#endif
    }

    public static byte[] FetchRaw(byte[] queryResult)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.FetchRaw(queryResult);
#else
        return MuduSys.WasmMuduSysCall.FetchRaw(queryResult);
#endif
    }

    public static byte[] FetchRaw(global::System.ReadOnlyMemory<byte> queryResult)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.FetchRaw(queryResult);
#else
        return MuduSys.WasmMuduSysCall.FetchRaw(queryResult);
#endif
    }

    public static byte[] SerializeCommand(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        return options is null
            ? MessagePackSerializer.Serialize(argv)
            : MessagePackSerializer.Serialize(argv, options);
    }

    public static byte[] SerializeQuery(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        return options is null
            ? MessagePackSerializer.Serialize(argv)
            : MessagePackSerializer.Serialize(argv, options);
    }

    public static UniCommandReturn DeserializeCommandResult(byte[] bytes, MessagePackSerializerOptions? options = null)
    {
        return options is null
            ? MessagePackSerializer.Deserialize<UniCommandReturn>(bytes)
            : MessagePackSerializer.Deserialize<UniCommandReturn>(bytes, options);
    }

    public static UniQueryReturn DeserializeQueryResult(byte[] bytes, MessagePackSerializerOptions? options = null)
    {
        return options is null
            ? MessagePackSerializer.Deserialize<UniQueryReturn>(bytes)
            : MessagePackSerializer.Deserialize<UniQueryReturn>(bytes, options);
    }

    public static UniCommandReturn SysCommand(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.SysCommand(argv, options);
#else
        var request = SerializeCommand(argv, options);
        var response = CommandRaw(request);
        return DeserializeCommandResult(response, options);
#endif
    }

    public static UniQueryReturn SysQuery(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
#if MUDU_MOCK_SQLITE
        return Mock.MockSqliteMuduSysCall.SysQuery(argv, options);
#else
        var request = SerializeQuery(argv, options);
        var response = QueryRaw(request);
        return DeserializeQueryResult(response, options);
#endif
    }

    public static ulong SysCommandAffectedRows(UniCommandArgv argv, MessagePackSerializerOptions? options = null)
    {
        var result = SysCommand(argv, options);
        return result.Kind() switch
        {
            UniCommandReturnKind.Ok => UniCommandReturnOk.AsOk(result).Inner.AffectedRows,
            UniCommandReturnKind.Err => throw new global::System.InvalidOperationException(UniCommandReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown command result kind"),
        };
    }

    public static UniQueryResult SysQueryOk(UniQueryArgv argv, MessagePackSerializerOptions? options = null)
    {
        var result = SysQuery(argv, options);
        return result.Kind() switch
        {
            UniQueryReturnKind.Ok => UniQueryReturnOk.AsOk(result).Inner,
            UniQueryReturnKind.Err => throw new global::System.InvalidOperationException(UniQueryReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown query result kind"),
        };
    }
}
