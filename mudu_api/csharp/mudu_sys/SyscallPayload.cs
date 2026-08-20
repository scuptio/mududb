#nullable enable

using System.Buffers;
using System.Buffers.Binary;
using MessagePack;

namespace Mudu.Api.MuduSys;

/// Syscall message kinds carried in the MSSP header's `message_kind` field.
/// The discriminants identify the 20 `uni-syscall.wit` functions and route
/// frames between guest and host. `0` and unknown values are rejected by
/// <see cref="SyscallPayload.DecodeFrame"/>.
public enum MessageKind : uint
{
    /// `query` — SQL query.
    Query = 1,

    /// `command` — SQL command.
    Command = 2,

    /// `batch` — batched SQL command.
    Batch = 3,

    /// `open-session` — open a KV session.
    Open = 4,

    /// `close-session` — close a KV session.
    Close = 5,

    /// `get` — KV point lookup.
    Get = 6,

    /// `put` — KV insert/update.
    Put = 7,

    /// `delete` — KV removal.
    Delete = 8,

    /// `range` — KV range scan.
    Range = 9,

    /// `fs-open` — open a file.
    FsOpen = 10,

    /// `fs-close` — close a file descriptor.
    FsClose = 11,

    /// `fs-read` — read at the current position.
    FsRead = 12,

    /// `fs-write` — write at the current position.
    FsWrite = 13,

    /// `fs-pread` — positional read.
    FsPread = 14,

    /// `fs-pwrite` — positional write.
    FsPwrite = 15,

    /// `fs-lseek` — reposition a file descriptor.
    FsLseek = 16,

    /// `fs-fstat` — stat an open file descriptor.
    FsFstat = 17,

    /// `fs-stat` — stat a path.
    FsStat = 18,

    /// `fs-fsync` — flush a file descriptor.
    FsFsync = 19,

    /// `fs-readdir` — list a directory.
    FsReaddir = 20,
}

/// Outcome of a decoded MSSP result body: either the success value or the
/// <see cref="UniError"/> carried by the `[1, error]` arm.
public readonly struct SyscallResult<T>
{
    private readonly T value;

    internal SyscallResult(T value)
    {
        this.value = value;
        Error = null;
    }

    internal SyscallResult(UniError error)
    {
        value = default!;
        Error = error;
    }

    public bool IsOk => Error is null;

    public bool IsErr => Error is not null;

    public T? Value => IsOk ? value : default;

    public UniError? Error { get; }
}

/// SyscallPayload v1 (MSSP) codec: the 16-byte header plus a MessagePack body
/// defined by `doc/cn/contract/syscall_payload_v1.md`, mirroring
/// `mudu_binding::codec::syscall_payload`.
///
/// Every syscall request and response is a self-describing frame:
///
/// ```text
/// +-------------------------------------------+
/// | Header (16 bytes, all fields big-endian)  |
/// |   offset  0: magic        = 0x4D535350    |
/// |   offset  4: version      = 1             |
/// |   offset  8: flags        = 0 (reserved)  |
/// |   offset 12: message_kind (MessageKind)   |
/// +-------------------------------------------+
/// | Body (single MessagePack value)           |
/// +-------------------------------------------+
/// ```
///
/// Request bodies are MessagePack arrays of the WIT-declared positional
/// arguments (a single argument is a one-element array; records nest as their
/// own array). Result bodies are `[ok_tag, value]` pairs with `0` = ok and
/// `1` = err; unit results use the `[0, 0]` placeholder form.
///
/// Error style: frame/header/structural violations (bad magic, unsupported
/// version, nonzero flags, unknown or mismatched kind, malformed bodies)
/// throw <see cref="global::System.IO.InvalidDataException"/> — they are
/// protocol errors, not business errors. Business errors carried in the
/// `[1, UniError]` result arm are returned as values.
public static class SyscallPayload
{
    /// Length in bytes of the fixed syscall payload header.
    public const int HeaderLen = 16;

    /// Frame magic: ASCII `MSSP`.
    public const uint Magic = 0x4D535350;

    /// The only payload format version this codec reads and writes.
    public const uint Version = 1;

    /// Encodes the fixed 16-byte header for `kind`.
    public static byte[] EncodeHeader(MessageKind kind)
    {
        var header = new byte[HeaderLen];
        BinaryPrimitives.WriteUInt32BigEndian(header.AsSpan(0, 4), Magic);
        BinaryPrimitives.WriteUInt32BigEndian(header.AsSpan(4, 4), Version);
        // flags at [8..12] stay zero.
        BinaryPrimitives.WriteUInt32BigEndian(header.AsSpan(12, 4), (uint)kind);
        return header;
    }

    /// Encodes a complete frame: the 16-byte header followed by `body`.
    public static byte[] EncodeFrame(MessageKind kind, global::System.ReadOnlySpan<byte> body)
    {
        var frame = new byte[HeaderLen + body.Length];
        EncodeHeader(kind).CopyTo(frame, 0);
        body.CopyTo(frame.AsSpan(HeaderLen));
        return frame;
    }

    /// Validates the header and returns the declared message kind.
    public static MessageKind DecodeHeader(global::System.ReadOnlyMemory<byte> frame)
    {
        if (frame.Length < HeaderLen)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP header shorter than {HeaderLen} bytes ({frame.Length})");
        }

        var span = frame.Span;
        var magic = BinaryPrimitives.ReadUInt32BigEndian(span.Slice(0, 4));
        if (magic != Magic)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP bad magic 0x{magic:X8}, expected 0x{Magic:X8}");
        }

        var version = BinaryPrimitives.ReadUInt32BigEndian(span.Slice(4, 4));
        if (version != Version)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP unsupported payload version {version}, supported range is [1, 1]");
        }

        var flags = BinaryPrimitives.ReadUInt32BigEndian(span.Slice(8, 4));
        if (flags != 0)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP nonzero header flags 0x{flags:X8}");
        }

        var kindRaw = BinaryPrimitives.ReadUInt32BigEndian(span.Slice(12, 4));
        if (kindRaw == 0 || !global::System.Enum.IsDefined((MessageKind)kindRaw))
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP unknown syscall message kind {kindRaw}");
        }

        return (MessageKind)kindRaw;
    }

    /// Splits a frame into its validated message kind and body.
    public static (MessageKind Kind, global::System.ReadOnlyMemory<byte> Body) DecodeFrame(
        global::System.ReadOnlyMemory<byte> frame)
    {
        var kind = DecodeHeader(frame);
        return (kind, frame.Slice(HeaderLen));
    }

    /// Validates a frame, checks it carries the expected message kind, and
    /// returns the body.
    public static global::System.ReadOnlyMemory<byte> ExpectFrame(
        MessageKind expected,
        global::System.ReadOnlyMemory<byte> frame)
    {
        var (kind, body) = DecodeFrame(frame);
        if (kind != expected)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP unexpected syscall message kind {(uint)kind}, expected {(uint)expected}");
        }

        return body;
    }

    // ---- request framing ----

    /// Encodes a request frame whose body is the 1-element MessagePack array
    /// `[arg]`. Record arguments nest as their own array, e.g. the `fs-open`
    /// body is `[[session, oid, path, flags]]`.
    public static byte[] EncodeRequestFrame<T1>(
        MessageKind kind,
        T1 arg1,
        MessagePackSerializerOptions? options = null)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(1);
        MessagePackSerializer.Serialize(ref writer, arg1, options);
        writer.Flush();
        return EncodeFrame(kind, buffer.WrittenSpan);
    }

    /// Encodes a request frame whose body is `[arg1, arg2]`.
    public static byte[] EncodeRequestFrame<T1, T2>(
        MessageKind kind,
        T1 arg1,
        T2 arg2,
        MessagePackSerializerOptions? options = null)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(2);
        MessagePackSerializer.Serialize(ref writer, arg1, options);
        MessagePackSerializer.Serialize(ref writer, arg2, options);
        writer.Flush();
        return EncodeFrame(kind, buffer.WrittenSpan);
    }

    /// Encodes a request frame whose body is `[arg1, arg2, arg3]`.
    public static byte[] EncodeRequestFrame<T1, T2, T3>(
        MessageKind kind,
        T1 arg1,
        T2 arg2,
        T3 arg3,
        MessagePackSerializerOptions? options = null)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(3);
        MessagePackSerializer.Serialize(ref writer, arg1, options);
        MessagePackSerializer.Serialize(ref writer, arg2, options);
        MessagePackSerializer.Serialize(ref writer, arg3, options);
        writer.Flush();
        return EncodeFrame(kind, buffer.WrittenSpan);
    }

    /// Decodes a request body holding a single positional argument.
    public static T1 DecodeRequestBody<T1>(
        global::System.ReadOnlyMemory<byte> body,
        MessagePackSerializerOptions? options = null)
    {
        var reader = new MessagePackReader(body);
        ReadArrayHeader(ref reader, 1);
        var arg1 = MessagePackSerializer.Deserialize<T1>(ref reader, options);
        EnsureConsumed(ref reader, body);
        return arg1!;
    }

    /// Decodes a request body holding two positional arguments.
    public static (T1, T2) DecodeRequestBody<T1, T2>(
        global::System.ReadOnlyMemory<byte> body,
        MessagePackSerializerOptions? options = null)
    {
        var reader = new MessagePackReader(body);
        ReadArrayHeader(ref reader, 2);
        var arg1 = MessagePackSerializer.Deserialize<T1>(ref reader, options);
        var arg2 = MessagePackSerializer.Deserialize<T2>(ref reader, options);
        EnsureConsumed(ref reader, body);
        return (arg1!, arg2!);
    }

    /// Decodes a request body holding three positional arguments.
    public static (T1, T2, T3) DecodeRequestBody<T1, T2, T3>(
        global::System.ReadOnlyMemory<byte> body,
        MessagePackSerializerOptions? options = null)
    {
        var reader = new MessagePackReader(body);
        ReadArrayHeader(ref reader, 3);
        var arg1 = MessagePackSerializer.Deserialize<T1>(ref reader, options);
        var arg2 = MessagePackSerializer.Deserialize<T2>(ref reader, options);
        var arg3 = MessagePackSerializer.Deserialize<T3>(ref reader, options);
        EnsureConsumed(ref reader, body);
        return (arg1!, arg2!, arg3!);
    }

    // ---- result framing ----

    /// Encodes a success result body: `[0, value]`.
    public static byte[] EncodeResultBody<T>(T value, MessagePackSerializerOptions? options = null)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(2);
        writer.Write((byte)0);
        MessagePackSerializer.Serialize(ref writer, value, options);
        writer.Flush();
        return buffer.WrittenSpan.ToArray();
    }

    /// Encodes an error result body: `[1, UniError]`.
    public static byte[] EncodeResultErrorBody(UniError error, MessagePackSerializerOptions? options = null)
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(2);
        writer.Write((byte)1);
        MessagePackSerializer.Serialize(ref writer, error, options);
        writer.Flush();
        return buffer.WrittenSpan.ToArray();
    }

    /// Encodes a unit success result body: `[0, 0]`.
    public static byte[] EncodeUnitResultBody()
    {
        var buffer = new ArrayBufferWriter<byte>();
        var writer = new MessagePackWriter(buffer);
        writer.WriteArrayHeader(2);
        writer.Write((byte)0);
        writer.Write((byte)0);
        writer.Flush();
        return buffer.WrittenSpan.ToArray();
    }

    /// Encodes a complete success result frame: header plus `[0, value]`.
    public static byte[] EncodeResultFrame<T>(MessageKind kind, T value, MessagePackSerializerOptions? options = null)
    {
        return EncodeFrame(kind, EncodeResultBody(value, options));
    }

    /// Encodes a complete error result frame: header plus `[1, UniError]`.
    public static byte[] EncodeResultErrorFrame(MessageKind kind, UniError error, MessagePackSerializerOptions? options = null)
    {
        return EncodeFrame(kind, EncodeResultErrorBody(error, options));
    }

    /// Encodes a complete unit success result frame: header plus `[0, 0]`.
    public static byte[] EncodeUnitResultFrame(MessageKind kind)
    {
        return EncodeFrame(kind, EncodeUnitResultBody());
    }

    /// Decodes a result frame body `[ok_tag, value]` into the success value
    /// or the carried <see cref="UniError"/>.
    public static SyscallResult<T> DecodeResultFrame<T>(
        MessageKind expected,
        byte[] frame,
        MessagePackSerializerOptions? options = null)
    {
        return DecodeResultBody<T>(ExpectFrame(expected, frame), options);
    }

    /// Decodes a result body `[ok_tag, value]`.
    public static SyscallResult<T> DecodeResultBody<T>(
        global::System.ReadOnlyMemory<byte> body,
        MessagePackSerializerOptions? options = null)
    {
        var reader = new MessagePackReader(body);
        ReadArrayHeader(ref reader, 2);
        var tag = reader.ReadByte();
        switch (tag)
        {
            case 0:
                var value = MessagePackSerializer.Deserialize<T>(ref reader, options);
                EnsureConsumed(ref reader, body);
                return new SyscallResult<T>(value!);
            case 1:
                var error = MessagePackSerializer.Deserialize<UniError>(ref reader, options);
                EnsureConsumed(ref reader, body);
                return new SyscallResult<T>(error);
            default:
                throw new global::System.IO.InvalidDataException(
                    $"MSSP unknown result tag {tag}, expected 0 (ok) or 1 (err)");
        }
    }

    /// Decodes a unit result frame (`[0, 0]` / `[1, UniError]`), returning
    /// the carried error or `null` on success.
    public static UniError? DecodeUnitResultFrame(
        MessageKind expected,
        byte[] frame,
        MessagePackSerializerOptions? options = null)
    {
        var body = ExpectFrame(expected, frame);
        var reader = new MessagePackReader(body);
        ReadArrayHeader(ref reader, 2);
        var tag = reader.ReadByte();
        switch (tag)
        {
            case 0:
                reader.Skip();
                EnsureConsumed(ref reader, body);
                return null;
            case 1:
                var error = MessagePackSerializer.Deserialize<UniError>(ref reader, options);
                EnsureConsumed(ref reader, body);
                return error;
            default:
                throw new global::System.IO.InvalidDataException(
                    $"MSSP unknown result tag {tag}, expected 0 (ok) or 1 (err)");
        }
    }

    private static void ReadArrayHeader(ref MessagePackReader reader, int expected)
    {
        var count = reader.ReadArrayHeader();
        if (count != expected)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP expected MessagePack array of length {expected}, got {count}");
        }
    }

    private static void EnsureConsumed(ref MessagePackReader reader, global::System.ReadOnlyMemory<byte> body)
    {
        if (reader.Consumed != body.Length)
        {
            throw new global::System.IO.InvalidDataException(
                $"MSSP trailing bytes after MessagePack body ({reader.Consumed} of {body.Length} consumed)");
        }
    }
}
