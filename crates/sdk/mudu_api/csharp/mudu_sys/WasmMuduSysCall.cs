#nullable enable

using ApiWorld.wit.imports.mududb.api;

namespace mududb.sys;

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

    public static byte[] BatchRaw(byte[] frame)
    {
        return ISystem.Batch(frame);
    }

    public static byte[] OpenRaw(byte[] frame)
    {
        return ISystem.Open(frame);
    }

    public static byte[] CloseRaw(byte[] frame)
    {
        return ISystem.Close(frame);
    }

    public static byte[] GetRaw(byte[] frame)
    {
        return ISystem.Get(frame);
    }

    public static byte[] PutRaw(byte[] frame)
    {
        return ISystem.Put(frame);
    }

    public static byte[] DeleteRaw(byte[] frame)
    {
        return ISystem.Delete(frame);
    }

    public static byte[] RangeRaw(byte[] frame)
    {
        return ISystem.Range(frame);
    }

    public static byte[] RelationGetRaw(byte[] frame)
    {
        return ISystem.RelationGet(frame);
    }

    public static byte[] RelationUpdateRaw(byte[] frame)
    {
        return ISystem.RelationUpdate(frame);
    }

    public static byte[] RelationInsertRaw(byte[] frame)
    {
        return ISystem.RelationInsert(frame);
    }
}
