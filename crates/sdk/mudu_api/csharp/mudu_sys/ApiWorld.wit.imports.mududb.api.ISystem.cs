// Generated-style bindings for `mududb:api/system`.
#nullable enable

namespace ApiWorld.wit.imports.mududb.api;

public interface ISystem
{
    internal static class QueryWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "query"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportQuery(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Query(byte[] queryIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(queryIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return QueryCore(listPtr, queryIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Query(global::System.ReadOnlyMemory<byte> queryIn)
    {
        fixed (void* listPtr = queryIn.Span)
        {
            return QueryCore((nint)listPtr, queryIn.Length);
        }
    }

    private static unsafe byte[] QueryCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        QueryWasmInterop.wasmImportQuery(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FetchWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fetch"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFetch(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Fetch(byte[] queryResult)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(queryResult, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FetchCore(listPtr, queryResult.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Fetch(global::System.ReadOnlyMemory<byte> queryResult)
    {
        fixed (void* listPtr = queryResult.Span)
        {
            return FetchCore((nint)listPtr, queryResult.Length);
        }
    }

    private static unsafe byte[] FetchCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FetchWasmInterop.wasmImportFetch(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class CommandWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "command"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportCommand(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Command(byte[] commandIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(commandIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return CommandCore(listPtr, commandIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Command(global::System.ReadOnlyMemory<byte> commandIn)
    {
        fixed (void* listPtr = commandIn.Span)
        {
            return CommandCore((nint)listPtr, commandIn.Length);
        }
    }

    private static unsafe byte[] CommandCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        CommandWasmInterop.wasmImportCommand(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsOpenWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-open"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsOpen(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsOpen(byte[] fsOpenIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsOpenIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsOpenCore(listPtr, fsOpenIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsOpen(global::System.ReadOnlyMemory<byte> fsOpenIn)
    {
        fixed (void* listPtr = fsOpenIn.Span)
        {
            return FsOpenCore((nint)listPtr, fsOpenIn.Length);
        }
    }

    private static unsafe byte[] FsOpenCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsOpenWasmInterop.wasmImportFsOpen(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsCloseWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-close"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsClose(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsClose(byte[] fsCloseIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsCloseIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsCloseCore(listPtr, fsCloseIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsClose(global::System.ReadOnlyMemory<byte> fsCloseIn)
    {
        fixed (void* listPtr = fsCloseIn.Span)
        {
            return FsCloseCore((nint)listPtr, fsCloseIn.Length);
        }
    }

    private static unsafe byte[] FsCloseCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsCloseWasmInterop.wasmImportFsClose(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsReadWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-read"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsRead(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsRead(byte[] fsReadIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsReadIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsReadCore(listPtr, fsReadIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsRead(global::System.ReadOnlyMemory<byte> fsReadIn)
    {
        fixed (void* listPtr = fsReadIn.Span)
        {
            return FsReadCore((nint)listPtr, fsReadIn.Length);
        }
    }

    private static unsafe byte[] FsReadCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsReadWasmInterop.wasmImportFsRead(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsWriteWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-write"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsWrite(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsWrite(byte[] fsWriteIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsWriteIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsWriteCore(listPtr, fsWriteIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsWrite(global::System.ReadOnlyMemory<byte> fsWriteIn)
    {
        fixed (void* listPtr = fsWriteIn.Span)
        {
            return FsWriteCore((nint)listPtr, fsWriteIn.Length);
        }
    }

    private static unsafe byte[] FsWriteCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsWriteWasmInterop.wasmImportFsWrite(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsPreadWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-pread"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsPread(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsPread(byte[] fsPreadIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsPreadIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsPreadCore(listPtr, fsPreadIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsPread(global::System.ReadOnlyMemory<byte> fsPreadIn)
    {
        fixed (void* listPtr = fsPreadIn.Span)
        {
            return FsPreadCore((nint)listPtr, fsPreadIn.Length);
        }
    }

    private static unsafe byte[] FsPreadCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsPreadWasmInterop.wasmImportFsPread(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsPwriteWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-pwrite"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsPwrite(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsPwrite(byte[] fsPwriteIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsPwriteIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsPwriteCore(listPtr, fsPwriteIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsPwrite(global::System.ReadOnlyMemory<byte> fsPwriteIn)
    {
        fixed (void* listPtr = fsPwriteIn.Span)
        {
            return FsPwriteCore((nint)listPtr, fsPwriteIn.Length);
        }
    }

    private static unsafe byte[] FsPwriteCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsPwriteWasmInterop.wasmImportFsPwrite(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsLseekWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-lseek"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsLseek(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsLseek(byte[] fsLseekIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsLseekIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsLseekCore(listPtr, fsLseekIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsLseek(global::System.ReadOnlyMemory<byte> fsLseekIn)
    {
        fixed (void* listPtr = fsLseekIn.Span)
        {
            return FsLseekCore((nint)listPtr, fsLseekIn.Length);
        }
    }

    private static unsafe byte[] FsLseekCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsLseekWasmInterop.wasmImportFsLseek(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsFstatWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-fstat"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsFstat(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsFstat(byte[] fsFstatIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsFstatIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsFstatCore(listPtr, fsFstatIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsFstat(global::System.ReadOnlyMemory<byte> fsFstatIn)
    {
        fixed (void* listPtr = fsFstatIn.Span)
        {
            return FsFstatCore((nint)listPtr, fsFstatIn.Length);
        }
    }

    private static unsafe byte[] FsFstatCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsFstatWasmInterop.wasmImportFsFstat(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsStatWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-stat"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsStat(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsStat(byte[] fsStatIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsStatIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsStatCore(listPtr, fsStatIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsStat(global::System.ReadOnlyMemory<byte> fsStatIn)
    {
        fixed (void* listPtr = fsStatIn.Span)
        {
            return FsStatCore((nint)listPtr, fsStatIn.Length);
        }
    }

    private static unsafe byte[] FsStatCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsStatWasmInterop.wasmImportFsStat(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsFsyncWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-fsync"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsFsync(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsFsync(byte[] fsFsyncIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsFsyncIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsFsyncCore(listPtr, fsFsyncIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsFsync(global::System.ReadOnlyMemory<byte> fsFsyncIn)
    {
        fixed (void* listPtr = fsFsyncIn.Span)
        {
            return FsFsyncCore((nint)listPtr, fsFsyncIn.Length);
        }
    }

    private static unsafe byte[] FsFsyncCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsFsyncWasmInterop.wasmImportFsFsync(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class FsReaddirWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "fs-readdir"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportFsReaddir(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] FsReaddir(byte[] fsReaddirIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(fsReaddirIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return FsReaddirCore(listPtr, fsReaddirIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] FsReaddir(global::System.ReadOnlyMemory<byte> fsReaddirIn)
    {
        fixed (void* listPtr = fsReaddirIn.Span)
        {
            return FsReaddirCore((nint)listPtr, fsReaddirIn.Length);
        }
    }

    private static unsafe byte[] FsReaddirCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        FsReaddirWasmInterop.wasmImportFsReaddir(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }


    internal static class BatchWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "batch"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportBatch(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Batch(byte[] batchIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(batchIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return BatchCore(listPtr, batchIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Batch(global::System.ReadOnlyMemory<byte> batchIn)
    {
        fixed (void* listPtr = batchIn.Span)
        {
            return BatchCore((nint)listPtr, batchIn.Length);
        }
    }

    private static unsafe byte[] BatchCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        BatchWasmInterop.wasmImportBatch(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class OpenWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "open"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportOpen(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Open(byte[] openIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(openIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return OpenCore(listPtr, openIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Open(global::System.ReadOnlyMemory<byte> openIn)
    {
        fixed (void* listPtr = openIn.Span)
        {
            return OpenCore((nint)listPtr, openIn.Length);
        }
    }

    private static unsafe byte[] OpenCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        OpenWasmInterop.wasmImportOpen(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class CloseWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "close"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportClose(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Close(byte[] closeIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(closeIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return CloseCore(listPtr, closeIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Close(global::System.ReadOnlyMemory<byte> closeIn)
    {
        fixed (void* listPtr = closeIn.Span)
        {
            return CloseCore((nint)listPtr, closeIn.Length);
        }
    }

    private static unsafe byte[] CloseCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        CloseWasmInterop.wasmImportClose(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class GetWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "get"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportGet(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Get(byte[] getIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(getIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return GetCore(listPtr, getIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Get(global::System.ReadOnlyMemory<byte> getIn)
    {
        fixed (void* listPtr = getIn.Span)
        {
            return GetCore((nint)listPtr, getIn.Length);
        }
    }

    private static unsafe byte[] GetCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        GetWasmInterop.wasmImportGet(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class PutWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "put"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportPut(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Put(byte[] putIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(putIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return PutCore(listPtr, putIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Put(global::System.ReadOnlyMemory<byte> putIn)
    {
        fixed (void* listPtr = putIn.Span)
        {
            return PutCore((nint)listPtr, putIn.Length);
        }
    }

    private static unsafe byte[] PutCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        PutWasmInterop.wasmImportPut(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class DeleteWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "delete"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportDelete(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Delete(byte[] deleteIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(deleteIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return DeleteCore(listPtr, deleteIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Delete(global::System.ReadOnlyMemory<byte> deleteIn)
    {
        fixed (void* listPtr = deleteIn.Span)
        {
            return DeleteCore((nint)listPtr, deleteIn.Length);
        }
    }

    private static unsafe byte[] DeleteCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        DeleteWasmInterop.wasmImportDelete(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class RangeWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "range"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportRange(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] Range(byte[] rangeIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(rangeIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return RangeCore(listPtr, rangeIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] Range(global::System.ReadOnlyMemory<byte> rangeIn)
    {
        fixed (void* listPtr = rangeIn.Span)
        {
            return RangeCore((nint)listPtr, rangeIn.Length);
        }
    }

    private static unsafe byte[] RangeCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        RangeWasmInterop.wasmImportRange(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class RelationGetWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "relation-get"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportRelationGet(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] RelationGet(byte[] relationGetIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(relationGetIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return RelationGetCore(listPtr, relationGetIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] RelationGet(global::System.ReadOnlyMemory<byte> relationGetIn)
    {
        fixed (void* listPtr = relationGetIn.Span)
        {
            return RelationGetCore((nint)listPtr, relationGetIn.Length);
        }
    }

    private static unsafe byte[] RelationGetCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        RelationGetWasmInterop.wasmImportRelationGet(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class RelationUpdateWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "relation-update"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportRelationUpdate(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] RelationUpdate(byte[] relationUpdateIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(relationUpdateIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return RelationUpdateCore(listPtr, relationUpdateIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] RelationUpdate(global::System.ReadOnlyMemory<byte> relationUpdateIn)
    {
        fixed (void* listPtr = relationUpdateIn.Span)
        {
            return RelationUpdateCore((nint)listPtr, relationUpdateIn.Length);
        }
    }

    private static unsafe byte[] RelationUpdateCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        RelationUpdateWasmInterop.wasmImportRelationUpdate(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    internal static class RelationInsertWasmInterop
    {
        [global::System.Runtime.InteropServices.DllImportAttribute("mududb:api/system", EntryPoint = "relation-insert"), global::System.Runtime.InteropServices.WasmImportLinkageAttribute]
        internal static extern void wasmImportRelationInsert(nint p0, int p1, nint p2);
    }

    public static unsafe byte[] RelationInsert(byte[] relationInsertIn)
    {
        var gcHandle = global::System.Runtime.InteropServices.GCHandle.Alloc(relationInsertIn, global::System.Runtime.InteropServices.GCHandleType.Pinned);
        try
        {
            var listPtr = gcHandle.AddrOfPinnedObject();
            return RelationInsertCore(listPtr, relationInsertIn.Length);
        }
        finally
        {
            gcHandle.Free();
        }
    }

    public static unsafe byte[] RelationInsert(global::System.ReadOnlyMemory<byte> relationInsertIn)
    {
        fixed (void* listPtr = relationInsertIn.Span)
        {
            return RelationInsertCore((nint)listPtr, relationInsertIn.Length);
        }
    }

    private static unsafe byte[] RelationInsertCore(nint listPtr, int length)
    {
        var retArea = stackalloc uint[3];
        RelationInsertWasmInterop.wasmImportRelationInsert(listPtr, length, (nint)retArea);
        return CopyAndFreeResult(retArea);
    }

    private static unsafe byte[] CopyAndFreeResult(uint* retArea)
    {
        var ptr = (nint)retArea[0];
        var len = checked((int)retArea[1]);

        if (len == 0)
        {
            return [];
        }

        var data = new byte[len];
        new global::System.ReadOnlySpan<byte>((void*)ptr, len).CopyTo(data);
        global::System.Runtime.InteropServices.NativeMemory.Free((void*)ptr);
        return data;
    }
}
