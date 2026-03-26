#nullable enable

using ApiWorld.wit.imports.mududb.api;

namespace Mudu.Api.MuduSys;

internal static class WasmMuduSysCall
{
    public static byte[] QueryRaw(byte[] queryIn)
    {
        return ISystem.Query(queryIn);
    }

    public static byte[] QueryRaw(global::System.ReadOnlyMemory<byte> queryIn)
    {
        return ISystem.Query(queryIn);
    }

    public static byte[] CommandRaw(byte[] commandIn)
    {
        return ISystem.Command(commandIn);
    }

    public static byte[] CommandRaw(global::System.ReadOnlyMemory<byte> commandIn)
    {
        return ISystem.Command(commandIn);
    }

    public static byte[] FetchRaw(byte[] queryResult)
    {
        return ISystem.Fetch(queryResult);
    }

    public static byte[] FetchRaw(global::System.ReadOnlyMemory<byte> queryResult)
    {
        return ISystem.Fetch(queryResult);
    }
}
