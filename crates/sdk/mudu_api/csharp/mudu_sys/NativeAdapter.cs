#nullable enable
using System;
using System.Runtime.InteropServices;
using System.Text;
namespace mududb.sys;

/// <summary>Explicit ABI v1 adapter selection. Configure MUDU_CONNECTION before loading.</summary>
public static class NativeAdapter
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Buffer { public IntPtr Data; public nuint Length; }
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate uint Version();
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int Invoke(byte[] operation, nuint opLength, byte[] frame, nuint length, out Buffer result);
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate void Free(Buffer result);
    private static Invoke? invoke;
    private static Free? free;
    private static readonly object gate = new();
    /// <summary>Whether the process has selected the real native adapter.</summary>
    public static bool Enabled => invoke is not null;
    /// <summary>Load the runtime once. The native library remains loaded for process lifetime.</summary>
    public static void Use(string? path = null)
    {
        lock (gate)
        {
            if (Enabled) throw new InvalidOperationException("Adapter already selected");
            path ??= Environment.GetEnvironmentVariable("MUDU_ADAPTER_LIBRARY");
            if (string.IsNullOrWhiteSpace(path)) throw new InvalidOperationException("Set MUDU_ADAPTER_LIBRARY to the extracted runtime library");
            var handle = NativeLibrary.Load(System.IO.Path.GetFullPath(path));
            try
            {
                var version = Marshal.GetDelegateForFunctionPointer<Version>(NativeLibrary.GetExport(handle, "mudu_adapter_abi_version"));
                if (version() != 1) throw new InvalidOperationException("Incompatible adapter ABI; ABI v1 required");
                var call = Marshal.GetDelegateForFunctionPointer<Invoke>(NativeLibrary.GetExport(handle, "mudu_adapter_call"));
                free = Marshal.GetDelegateForFunctionPointer<Free>(NativeLibrary.GetExport(handle, "mudu_adapter_free"));
                invoke = call;
            }
            catch { NativeLibrary.Free(handle); throw; }
        }
    }
    /// <summary>Send an MSSP frame and release the native result after copying it.</summary>
    public static byte[] Call(string operation, ReadOnlyMemory<byte> frame)
    {
        var call = invoke ?? throw new InvalidOperationException("Select NativeAdapter.Use first");
        var name = Encoding.ASCII.GetBytes(operation);
        var bytes = frame.ToArray();
        int status = call(name, (nuint)name.Length, bytes, (nuint)bytes.Length, out var result);
        byte[] output;
        try
        {
            output = new byte[checked((int)result.Length)];
            if (output.Length != 0) Marshal.Copy(result.Data, output, 0, output.Length);
        }
        finally { free!(result); }
        if (status != 0) throw new InvalidOperationException("Native adapter: " + Encoding.UTF8.GetString(output));
        return output;
    }
}
