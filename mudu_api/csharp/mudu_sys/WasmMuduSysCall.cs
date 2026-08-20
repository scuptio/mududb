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

    // The fs family transports complete MSSP frames over the same byte pipe;
    // the frames are opaque to these forwarders.

    public static byte[] FsOpenRaw(byte[] frame)
    {
        return ISystem.FsOpen(frame);
    }

    public static byte[] FsCloseRaw(byte[] frame)
    {
        return ISystem.FsClose(frame);
    }

    public static byte[] FsReadRaw(byte[] frame)
    {
        return ISystem.FsRead(frame);
    }

    public static byte[] FsWriteRaw(byte[] frame)
    {
        return ISystem.FsWrite(frame);
    }

    public static byte[] FsPreadRaw(byte[] frame)
    {
        return ISystem.FsPread(frame);
    }

    public static byte[] FsPwriteRaw(byte[] frame)
    {
        return ISystem.FsPwrite(frame);
    }

    public static byte[] FsLseekRaw(byte[] frame)
    {
        return ISystem.FsLseek(frame);
    }

    public static byte[] FsFstatRaw(byte[] frame)
    {
        return ISystem.FsFstat(frame);
    }

    public static byte[] FsStatRaw(byte[] frame)
    {
        return ISystem.FsStat(frame);
    }

    public static byte[] FsFsyncRaw(byte[] frame)
    {
        return ISystem.FsFsync(frame);
    }

    public static byte[] FsReaddirRaw(byte[] frame)
    {
        return ISystem.FsReaddir(frame);
    }
}
