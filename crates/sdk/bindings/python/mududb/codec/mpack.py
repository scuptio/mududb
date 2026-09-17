"""MessagePack runtime used by the mgen-generated Python codecs.

Encoding is byte-exact with rmp-serde 1.3.x (``rmp_serde::to_vec``):

- Integers use minimal-width encoding BY VALUE, independent of the source
  type. Non-negative values use the unsigned marker chain (fixint / 0xCC u8 /
  0xCD u16 / 0xCE u32 / 0xCF u64); negative values use negfixint / 0xD0 i8 /
  0xD1 i16 / 0xD2 i32 / 0xD3 i64.
- Strings use fixstr / str8 (0xD9) / str16 (0xDA) / str32 (0xDB); rmp_serde
  DOES emit str8 for lengths 32..=255.
- Binary uses bin8 (0xC4) / bin16 (0xC5) / bin32 (0xC6).
- Arrays use fixarray / array16 (0xDC) / array32 (0xDD); maps use fixmap /
  map16 (0xDE) / map32 (0xDF). Map keys encode recursively.
- Plain ``float`` values encode as f64 (0xCB). The :class:`F32` marker class
  makes the writer emit f32 (0xCA) instead.
- Decoding floats is lenient across the 0xCA/0xCB markers in both directions,
  matching ``rmp_serde::from_slice``.

Record/request bodies are integer-keyed maps (MSSP v1 revised wire format):
:meth:`MpackReader.read_map_key` mirrors the host's lenient key handling
(integer keys of any width, negative or non-integer keys collapse to the
never-used key 0 so the caller skips the value) and
:meth:`MpackReader.skip_value` consumes and discards any value.

Pure stdlib, Python 3.9+.
"""

import struct

__all__ = ["F32", "MpackReader", "MpackWriter"]

_U64_MAX = 0xFFFFFFFFFFFFFFFF
_I64_MIN = -0x8000000000000000
_I64_MAX = 0x7FFFFFFFFFFFFFFF


class F32(float):
    """Marker subclass of ``float``: the writer emits f32 (0xCA + big-endian
    IEEE-754 single) for it, while a plain ``float`` encodes as f64 (0xCB).

    Python floats are doubles; construct with an already f32-truncated value
    (e.g. ``F32(struct.unpack('>f', struct.pack('>f', v))[0])``) when the exact
    wire value matters. The writer truncates to single precision on encode
    regardless.
    """


def _f32_bits(v):
    return struct.pack(">f", v)


class MpackWriter:
    """Canonical (rmp_serde-compatible) MessagePack writer."""

    def __init__(self):
        self._buf = bytearray()

    def to_bytes(self):
        """Return the encoded bytes accumulated so far."""
        return bytes(self._buf)

    # ---- typed writers ----

    def write_nil(self):
        self._buf.append(0xC0)

    def write_bool(self, v):
        self._buf.append(0xC3 if v else 0xC2)

    def write_u64(self, v):
        if isinstance(v, bool) or not isinstance(v, int):
            raise TypeError(f"write_u64 expects int, got {type(v).__name__}")
        if v < 0 or v > _U64_MAX:
            raise ValueError(f"u64 out of range [0, {_U64_MAX}]: {v}")
        self._write_unsigned(v)

    def write_i64(self, v):
        if isinstance(v, bool) or not isinstance(v, int):
            raise TypeError(f"write_i64 expects int, got {type(v).__name__}")
        if v < _I64_MIN or v > _I64_MAX:
            raise ValueError(f"i64 out of range [{_I64_MIN}, {_I64_MAX}]: {v}")
        if v >= 0:
            # non-negative i64 goes through the unsigned chain (rmp_serde
            # encodes by value, not by declared type)
            self._write_unsigned(v)
        else:
            self._write_negative(v)

    def write_f32(self, v):
        self._buf.append(0xCA)
        self._buf += _f32_bits(v)

    def write_f64(self, v):
        self._buf.append(0xCB)
        self._buf += struct.pack(">d", v)

    def write_str(self, s):
        if not isinstance(s, str):
            raise TypeError(f"write_str expects str, got {type(s).__name__}")
        data = s.encode("utf-8")
        n = len(data)
        if n <= 31:
            self._buf.append(0xA0 | n)
        elif n <= 0xFF:
            self._buf += bytes((0xD9, n))
        elif n <= 0xFFFF:
            self._buf.append(0xDA)
            self._buf += struct.pack(">H", n)
        else:
            self._buf.append(0xDB)
            self._buf += struct.pack(">I", n)
        self._buf += data

    def write_bin(self, b):
        if not isinstance(b, (bytes, bytearray)):
            raise TypeError(f"write_bin expects bytes, got {type(b).__name__}")
        n = len(b)
        if n <= 0xFF:
            self._buf += bytes((0xC4, n))
        elif n <= 0xFFFF:
            self._buf.append(0xC5)
            self._buf += struct.pack(">H", n)
        else:
            self._buf.append(0xC6)
            self._buf += struct.pack(">I", n)
        self._buf += b

    def write_array_header(self, n):
        if n < 0 or n > 0xFFFFFFFF:
            raise ValueError(f"array length out of range: {n}")
        if n <= 15:
            self._buf.append(0x90 | n)
        elif n <= 0xFFFF:
            self._buf.append(0xDC)
            self._buf += struct.pack(">H", n)
        else:
            self._buf.append(0xDD)
            self._buf += struct.pack(">I", n)

    def write_map_header(self, n):
        if n < 0 or n > 0xFFFFFFFF:
            raise ValueError(f"map length out of range: {n}")
        if n <= 15:
            self._buf.append(0x80 | n)
        elif n <= 0xFFFF:
            self._buf.append(0xDE)
            self._buf += struct.pack(">H", n)
        else:
            self._buf.append(0xDF)
            self._buf += struct.pack(">I", n)

    # ---- generic canonical writer ----

    def write_value(self, v):
        """Recursively write a native Python value in canonical form."""
        if v is None:
            self.write_nil()
        elif isinstance(v, bool):
            # bool is an int subclass: it must be checked before int and F32
            self.write_bool(v)
        elif isinstance(v, F32):
            self.write_f32(v)
        elif isinstance(v, int):
            if v >= 0:
                if v > _U64_MAX:
                    raise ValueError(f"integer too large for MessagePack: {v}")
                self._write_unsigned(v)
            else:
                if v < _I64_MIN:
                    raise ValueError(f"integer too small for MessagePack: {v}")
                self._write_negative(v)
        elif isinstance(v, float):
            self.write_f64(v)
        elif isinstance(v, str):
            self.write_str(v)
        elif isinstance(v, (bytes, bytearray)):
            self.write_bin(v)
        elif isinstance(v, (list, tuple)):
            self.write_array_header(len(v))
            for item in v:
                self.write_value(item)
        elif isinstance(v, dict):
            self.write_map_header(len(v))
            for key, item in v.items():
                self.write_value(key)
                self.write_value(item)
        else:
            raise TypeError(f"unsupported type for MessagePack: {type(v).__name__}")

    # ---- internals ----

    def _write_unsigned(self, v):
        if v <= 0x7F:
            self._buf.append(v)
        elif v <= 0xFF:
            self._buf += bytes((0xCC, v))
        elif v <= 0xFFFF:
            self._buf.append(0xCD)
            self._buf += struct.pack(">H", v)
        elif v <= 0xFFFFFFFF:
            self._buf.append(0xCE)
            self._buf += struct.pack(">I", v)
        else:
            self._buf.append(0xCF)
            self._buf += struct.pack(">Q", v)

    def _write_negative(self, v):
        if v >= -32:
            self._buf.append(v & 0xFF)
        elif v >= -128:
            self._buf.append(0xD0)
            self._buf += struct.pack(">b", v)
        elif v >= -32768:
            self._buf.append(0xD1)
            self._buf += struct.pack(">h", v)
        elif v >= -2147483648:
            self._buf.append(0xD2)
            self._buf += struct.pack(">i", v)
        else:
            self._buf.append(0xD3)
            self._buf += struct.pack(">q", v)


class MpackReader:
    """Lenient MessagePack reader over a ``bytes`` buffer."""

    def __init__(self, data):
        self._data = bytes(data)
        self._pos = 0

    def is_done(self):
        """True when every input byte has been consumed."""
        return self._pos == len(self._data)

    # ---- generic lenient reader ----

    def read_value(self):
        """Read one value of any shape into native Python objects: any integer
        width -> int, 0xCA/0xCB -> float, bin -> bytes, str -> str, array ->
        list, map -> dict."""
        b = self._take()
        if b <= 0x7F:
            return b
        if b >= 0xE0:
            return b - 0x100
        if (b & 0xF0) == 0x80:
            return self._read_map(b & 0x0F)
        if (b & 0xF0) == 0x90:
            return self._read_array(b & 0x0F)
        if (b & 0xE0) == 0xA0:
            return self._take_str(b & 0x1F)
        if b == 0xC0:
            return None
        if b == 0xC2:
            return False
        if b == 0xC3:
            return True
        if b == 0xC4:
            return self._take_bin(self._take())
        if b == 0xC5:
            return self._take_bin(self._take_be(">H"))
        if b == 0xC6:
            return self._take_bin(self._take_be(">I"))
        if b == 0xCA:
            return self._take_be(">f")
        if b == 0xCB:
            return self._take_be(">d")
        if b == 0xCC:
            return self._take()
        if b == 0xCD:
            return self._take_be(">H")
        if b == 0xCE:
            return self._take_be(">I")
        if b == 0xCF:
            return self._take_be(">Q")
        if b == 0xD0:
            return self._take_be(">b")
        if b == 0xD1:
            return self._take_be(">h")
        if b == 0xD2:
            return self._take_be(">i")
        if b == 0xD3:
            return self._take_be(">q")
        if b == 0xD9:
            return self._take_str(self._take())
        if b == 0xDA:
            return self._take_str(self._take_be(">H"))
        if b == 0xDB:
            return self._take_str(self._take_be(">I"))
        if b == 0xDC:
            return self._read_array(self._take_be(">H"))
        if b == 0xDD:
            return self._read_array(self._take_be(">I"))
        if b == 0xDE:
            return self._read_map(self._take_be(">H"))
        if b == 0xDF:
            return self._read_map(self._take_be(">I"))
        raise ValueError(f"mpack: unknown marker 0x{b:02x}")

    # ---- typed readers (lenient across integer widths) ----

    def is_nil(self):
        """Peek: True when the next byte is the nil marker (not consumed)."""
        return self._pos < len(self._data) and self._data[self._pos] == 0xC0

    def read_nil(self):
        b = self._take()
        if b != 0xC0:
            raise ValueError(f"mpack: expected nil marker, got 0x{b:02x}")

    def read_bool(self):
        b = self._take()
        if b == 0xC2:
            return False
        if b == 0xC3:
            return True
        raise ValueError(f"mpack: expected bool marker, got 0x{b:02x}")

    def read_u64(self):
        b = self._take()
        if b <= 0x7F:
            return b
        if b == 0xCC:
            return self._take()
        if b == 0xCD:
            return self._take_be(">H")
        if b == 0xCE:
            return self._take_be(">I")
        if b == 0xCF:
            return self._take_be(">Q")
        raise ValueError(f"mpack: expected unsigned int marker, got 0x{b:02x}")

    def read_i64(self):
        b = self._take()
        if b <= 0x7F:
            return b
        if b >= 0xE0:
            return b - 0x100
        if b == 0xCC:
            return self._take()
        if b == 0xCD:
            return self._take_be(">H")
        if b == 0xCE:
            return self._take_be(">I")
        if b == 0xCF:
            v = self._take_be(">Q")
            if v > _I64_MAX:
                raise ValueError("mpack: u64 value does not fit into i64")
            return v
        if b == 0xD0:
            return self._take_be(">b")
        if b == 0xD1:
            return self._take_be(">h")
        if b == 0xD2:
            return self._take_be(">i")
        if b == 0xD3:
            return self._take_be(">q")
        raise ValueError(f"mpack: expected int marker, got 0x{b:02x}")

    def read_f32(self):
        b = self._take()
        # rmp_serde decodes f32 from either marker (see module docstring)
        if b == 0xCA:
            return self._take_be(">f")
        if b == 0xCB:
            # truncate the f64 wire value to single precision
            return struct.unpack(">f", _f32_bits(self._take_be(">d")))[0]
        raise ValueError(f"mpack: expected f32 marker, got 0x{b:02x}")

    def read_f64(self):
        b = self._take()
        if b == 0xCB:
            return self._take_be(">d")
        if b == 0xCA:
            # f32 -> f64 is exact
            return self._take_be(">f")
        raise ValueError(f"mpack: expected f64 marker, got 0x{b:02x}")

    def read_str(self):
        b = self._take()
        if (b & 0xE0) == 0xA0:
            n = b & 0x1F
        elif b == 0xD9:
            n = self._take()
        elif b == 0xDA:
            n = self._take_be(">H")
        elif b == 0xDB:
            n = self._take_be(">I")
        else:
            raise ValueError(f"mpack: expected str marker, got 0x{b:02x}")
        return self._take_str(n)

    def read_bin(self):
        b = self._take()
        if b == 0xC4:
            n = self._take()
        elif b == 0xC5:
            n = self._take_be(">H")
        elif b == 0xC6:
            n = self._take_be(">I")
        else:
            raise ValueError(f"mpack: expected bin marker, got 0x{b:02x}")
        return self._take_bin(n)

    def read_array_header(self):
        b = self._take()
        if (b & 0xF0) == 0x90:
            return b & 0x0F
        if b == 0xDC:
            return self._take_be(">H")
        if b == 0xDD:
            return self._take_be(">I")
        raise ValueError(f"mpack: expected array marker, got 0x{b:02x}")

    def read_map_header(self):
        b = self._take()
        if (b & 0xF0) == 0x80:
            return b & 0x0F
        if b == 0xDE:
            return self._take_be(">H")
        if b == 0xDF:
            return self._take_be(">I")
        raise ValueError(f"mpack: expected map marker, got 0x{b:02x}")

    def read_map_key(self):
        """Read a record/request map key. Integer keys of any width are
        returned (negative keys collapse to 0); any non-integer key is
        consumed and also reported as 0. Field numbers are 1-based, so 0
        tells the caller to skip the associated value — matching the host's
        lenient key handling."""
        if self._pos >= len(self._data):
            raise ValueError("mpack: unexpected end of input")
        b = self._data[self._pos]
        is_int = b <= 0x7F or b >= 0xE0 or 0xCC <= b <= 0xD3
        if not is_int:
            self.skip_value()
            return 0
        v = self.read_i64()
        return v if v >= 0 else 0

    def skip_value(self):
        """Consume one MessagePack value of any shape and discard it (used for
        unknown record/request map keys)."""
        b = self._take()
        if b <= 0x7F or b >= 0xE0:
            return  # fixint
        if b in (0xC0, 0xC2, 0xC3):
            return  # nil / bool
        if b in (0xCC, 0xD0):
            self._skip_bytes(1)
            return
        if b in (0xCD, 0xD1):
            self._skip_bytes(2)
            return
        if b in (0xCA, 0xCE, 0xD2):
            self._skip_bytes(4)
            return
        if b in (0xCB, 0xCF, 0xD3):
            self._skip_bytes(8)
            return
        if (b & 0xE0) == 0xA0:
            self._skip_bytes(b & 0x1F)
            return
        if b in (0xD9, 0xC4):
            self._skip_bytes(self._take())
            return
        if b in (0xDA, 0xC5):
            self._skip_bytes(self._take_be(">H"))
            return
        if b in (0xDB, 0xC6):
            self._skip_bytes(self._take_be(">I"))
            return
        if (b & 0xF0) == 0x90:
            self._skip_items(b & 0x0F)
            return
        if (b & 0xF0) == 0x80:
            self._skip_items((b & 0x0F) * 2)
            return
        if b == 0xDC:
            self._skip_items(self._take_be(">H"))
            return
        if b == 0xDD:
            self._skip_items(self._take_be(">I"))
            return
        if b == 0xDE:
            self._skip_items(self._take_be(">H") * 2)
            return
        if b == 0xDF:
            self._skip_items(self._take_be(">I") * 2)
            return
        raise ValueError(f"mpack: cannot skip marker 0x{b:02x}")

    # ---- internals ----

    def _read_array(self, n):
        return [self.read_value() for _ in range(n)]

    def _read_map(self, n):
        out = {}
        for _ in range(n):
            key = self.read_value()
            value = self.read_value()
            try:
                out[key] = value
            except TypeError:
                raise ValueError(f"mpack: unhashable map key {key!r}") from None
        return out

    def _skip_items(self, n):
        for _ in range(n):
            self.skip_value()

    def _skip_bytes(self, n):
        if self._pos + n > len(self._data):
            raise ValueError("mpack: unexpected end of input")
        self._pos += n

    def _take(self):
        if self._pos >= len(self._data):
            raise ValueError("mpack: unexpected end of input")
        b = self._data[self._pos]
        self._pos += 1
        return b

    def _take_be(self, fmt):
        n = struct.calcsize(fmt)
        if self._pos + n > len(self._data):
            raise ValueError("mpack: unexpected end of input")
        v = struct.unpack_from(fmt, self._data, self._pos)[0]
        self._pos += n
        return v

    def _take_str(self, n):
        if self._pos + n > len(self._data):
            raise ValueError("mpack: unexpected end of input")
        raw = self._data[self._pos : self._pos + n]
        self._pos += n
        try:
            return raw.decode("utf-8")
        except UnicodeDecodeError as e:
            raise ValueError(f"mpack: invalid UTF-8 string: {e}") from None

    def _take_bin(self, n):
        if self._pos + n > len(self._data):
            raise ValueError("mpack: unexpected end of input")
        out = self._data[self._pos : self._pos + n]
        self._pos += n
        return out
