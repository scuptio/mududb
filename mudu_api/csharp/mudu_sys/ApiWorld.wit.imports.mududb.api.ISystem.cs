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
