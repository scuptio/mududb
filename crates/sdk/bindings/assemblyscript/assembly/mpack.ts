// MessagePack runtime used by mgen-generated AssemblyScript codecs.
//
// Encoding is byte-exact with rmp-serde 1.3.x (`rmp_serde::to_vec`):
// - Integers use minimal-width encoding BY VALUE, independent of the Rust
//   source type (verified: rmp_serde encodes `2u8`/`2u16`/`2u32`/`2u64` all
//   as the single fixint 0x02).
// - Non-negative values use the unsigned marker chain
//   (fixint / 0xCC u8 / 0xCD u16 / 0xCE u32 / 0xCF u64); negative values use
//   negfixint / 0xD0 i8 / 0xD1 i16 / 0xD2 i32 / 0xD3 i64.
// - Strings use fixstr / str8 (0xD9) / str16 (0xDA) / str32 (0xDB);
//   rmp_serde DOES emit str8 for lengths 32..=255.
// - Binary uses bin8 (0xC4) / bin16 (0xC5) / bin32 (0xC6).
// - Arrays use fixarray / array16 (0xDC) / array32 (0xDD); maps use fixmap /
//   map16 (0xDE) / map32 (0xDF).
// - Decoding floats is lenient across the 0xCA/0xCB markers in both
//   directions, matching `rmp_serde::from_slice` (`from_slice::<f32>` accepts
//   an f64 encoding and vice versa).
// - Record/request bodies are integer-keyed maps (MSSP v1 revised wire
//   format): `readMapKey` mirrors the host's lenient key handling (integer
//   keys of any width, negative or non-integer keys collapse to the
//   never-used key 0 so the caller skips the value) and `skipValue` consumes
//   and discards any value (unknown map keys).
// - `readBool` additionally accepts the integer 0/1 form: a bool crosses the
//   Mudu procedure byte pipe as an i32 (the host's uni-data-value vocabulary
//   has no Bool case), so record codecs decoding host-supplied records must
//   accept it (the generated decoders call `readBool` for bool fields).

export class MpackWriter {
  private buf: Uint8Array;
  private len: i32 = 0;

  constructor(capacity: i32 = 256) {
    this.buf = new Uint8Array(capacity);
  }

  writeNil(): void {
    this.push(0xc0);
  }

  writeBool(v: bool): void {
    this.push(v ? 0xc3 : 0xc2);
  }

  writeU64(v: u64): void {
    if (v <= 0x7f) {
      this.push(<u8>v);
    } else if (v <= 0xff) {
      this.push(0xcc);
      this.push(<u8>v);
    } else if (v <= 0xffff) {
      this.push(0xcd);
      this.pushBe16(v);
    } else if (v <= 0xffffffff) {
      this.push(0xce);
      this.pushBe32(v);
    } else {
      this.push(0xcf);
      this.pushBe64(v);
    }
  }

  writeI64(v: i64): void {
    if (v >= 0) {
      this.writeU64(<u64>v);
    } else if (v >= -32) {
      this.push(<u8>(v & 0xff));
    } else if (v >= -128) {
      this.push(0xd0);
      this.push(<u8>(v & 0xff));
    } else if (v >= -32768) {
      this.push(0xd1);
      this.pushBe16(<u64>(v & 0xffff));
    } else if (v >= -2147483648) {
      this.push(0xd2);
      this.pushBe32(<u64>(v & 0xffffffff));
    } else {
      this.push(0xd3);
      this.pushBe64(<u64>v);
    }
  }

  writeF32(v: f32): void {
    this.push(0xca);
    this.pushBe32(reinterpret<u32>(v));
  }

  writeF64(v: f64): void {
    this.push(0xcb);
    this.pushBe64(reinterpret<u64>(v));
  }

  writeString(s: string): void {
    const bytes = String.UTF8.encode(s, false);
    const n = bytes.byteLength;
    if (n <= 31) {
      this.push(<u8>(0xa0 | n));
    } else if (n <= 255) {
      this.push(0xd9);
      this.push(<u8>n);
    } else if (n <= 65535) {
      this.push(0xda);
      this.pushBe16(<u64>n);
    } else {
      this.push(0xdb);
      this.pushBe32(<u64>n);
    }
    this.pushBytes(changetype<usize>(bytes), n);
  }

  writeBin(b: Uint8Array): void {
    const n = b.length;
    if (n <= 255) {
      this.push(0xc4);
      this.push(<u8>n);
    } else if (n <= 65535) {
      this.push(0xc5);
      this.pushBe16(<u64>n);
    } else {
      this.push(0xc6);
      this.pushBe32(<u64>n);
    }
    this.pushBytes(changetype<usize>(b.buffer) + b.byteOffset, n);
  }

  writeArrayHeader(len: u32): void {
    if (len <= 15) {
      this.push(<u8>(0x90 | len));
    } else if (len <= 65535) {
      this.push(0xdc);
      this.pushBe16(len);
    } else {
      this.push(0xdd);
      this.pushBe32(len);
    }
  }

  writeMapHeader(len: u32): void {
    if (len <= 15) {
      this.push(<u8>(0x80 | len));
    } else if (len <= 65535) {
      this.push(0xde);
      this.pushBe16(len);
    } else {
      this.push(0xdf);
      this.pushBe32(len);
    }
  }

  toBytes(): Uint8Array {
    const out = new Uint8Array(this.len);
    if (this.len > 0) {
      memory.copy(
        changetype<usize>(out.buffer) + out.byteOffset,
        changetype<usize>(this.buf.buffer) + this.buf.byteOffset,
        this.len,
      );
    }
    return out;
  }

  private push(b: u8): void {
    this.ensure(1);
    this.buf[this.len] = b;
    this.len += 1;
  }

  private pushBe16(v: u64): void {
    this.push(<u8>(v >> 8));
    this.push(<u8>v);
  }

  private pushBe32(v: u64): void {
    this.push(<u8>(v >> 24));
    this.push(<u8>(v >> 16));
    this.push(<u8>(v >> 8));
    this.push(<u8>v);
  }

  private pushBe64(v: u64): void {
    this.push(<u8>(v >> 56));
    this.push(<u8>(v >> 48));
    this.push(<u8>(v >> 40));
    this.push(<u8>(v >> 32));
    this.push(<u8>(v >> 24));
    this.push(<u8>(v >> 16));
    this.push(<u8>(v >> 8));
    this.push(<u8>v);
  }

  private pushBytes(ptr: usize, n: i32): void {
    if (n == 0) return;
    this.ensure(n);
    memory.copy(
      changetype<usize>(this.buf.buffer) + this.buf.byteOffset + this.len,
      ptr,
      n,
    );
    this.len += n;
  }

  private ensure(n: i32): void {
    if (this.len + n <= this.buf.length) return;
    let cap = this.buf.length * 2;
    while (cap < this.len + n) cap *= 2;
    const next = new Uint8Array(cap);
    memory.copy(
      changetype<usize>(next.buffer) + next.byteOffset,
      changetype<usize>(this.buf.buffer) + this.buf.byteOffset,
      this.len,
    );
    this.buf = next;
  }
}

export class MpackReader {
  private bytes: Uint8Array;
  private pos: i32 = 0;

  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  tryNil(): bool {
    if (this.pos < this.bytes.length && this.bytes[this.pos] == 0xc0) {
      this.pos += 1;
      return true;
    }
    return false;
  }

  readBool(): bool {
    const b = this.take();
    if (b == 0xc2) return false;
    if (b == 0xc3) return true;
    // A bool crosses the procedure byte pipe as an i32 (0/1): accept the
    // integer form so the generated record codecs decode host-supplied
    // records (see the module header).
    this.pos -= 1;
    if (this.isIntMarker(this.bytes[this.pos])) {
      const v = this.readI64();
      if (v == 0) return false;
      if (v == 1) return true;
      throw new Error("mpack: integer value " + v.toString() + " is not a bool (0/1)");
    }
    throw new Error("mpack: expected bool marker, got 0x" + b.toString(16));
  }

  // The next MessagePack marker byte without consuming it (used by the
  // record bridge's schema-less value parse).
  peekCode(): u8 {
    if (this.pos >= this.bytes.length) {
      throw new Error("mpack: unexpected end of input");
    }
    return this.bytes[this.pos];
  }

  private isIntMarker(b: u8): bool {
    return b <= 0x7f || b >= 0xe0 || (b >= 0xcc && b <= 0xd3);
  }

  readU64(): u64 {
    const b = this.take();
    if (b <= 0x7f) return b;
    if (b == 0xcc) return this.take();
    if (b == 0xcd) return this.takeBe16();
    if (b == 0xce) return this.takeBe32();
    if (b == 0xcf) return this.takeBe64();
    throw new Error("mpack: expected unsigned int marker, got 0x" + b.toString(16));
  }

  readI64(): i64 {
    const b = this.take();
    if (b <= 0x7f) return b;
    if (b >= 0xe0) return <i8>b;
    if (b == 0xcc) return this.take();
    if (b == 0xcd) return this.takeBe16();
    if (b == 0xce) return this.takeBe32();
    if (b == 0xcf) {
      const v = this.takeBe64();
      if (v > <u64>i64.MAX_VALUE) {
        throw new Error("mpack: u64 value does not fit into i64");
      }
      return <i64>v;
    }
    if (b == 0xd0) return <i8>this.take();
    if (b == 0xd1) return <i16>this.takeBe16();
    if (b == 0xd2) return <i32>this.takeBe32();
    if (b == 0xd3) return <i64>this.takeBe64();
    throw new Error("mpack: expected int marker, got 0x" + b.toString(16));
  }

  readF32(): f32 {
    const b = this.take();
    // rmp_serde decodes f32 from either marker (see module header).
    if (b == 0xca) return reinterpret<f32>(<u32>this.takeBe32());
    if (b == 0xcb) return <f32>reinterpret<f64>(this.takeBe64());
    throw new Error("mpack: expected f32 marker, got 0x" + b.toString(16));
  }

  readF64(): f64 {
    const b = this.take();
    // rmp_serde decodes f64 from either marker (see module header).
    if (b == 0xcb) return reinterpret<f64>(this.takeBe64());
    if (b == 0xca) return <f64>reinterpret<f32>(<u32>this.takeBe32());
    throw new Error("mpack: expected f64 marker, got 0x" + b.toString(16));
  }

  readString(): string {
    const b = this.take();
    let n: u32;
    if ((b & 0xe0) == 0xa0) {
      n = b & 0x1f;
    } else if (b == 0xd9) {
      n = this.take();
    } else if (b == 0xda) {
      n = <u32>this.takeBe16();
    } else if (b == 0xdb) {
      n = <u32>this.takeBe32();
    } else {
      throw new Error("mpack: expected str marker, got 0x" + b.toString(16));
    }
    const s = String.UTF8.decodeUnsafe(this.regionPtr(<i32>n), <i32>n, false);
    this.pos += <i32>n;
    return s;
  }

  readBin(): Uint8Array {
    const b = this.take();
    let n: u32;
    if (b == 0xc4) {
      n = this.take();
    } else if (b == 0xc5) {
      n = <u32>this.takeBe16();
    } else if (b == 0xc6) {
      n = <u32>this.takeBe32();
    } else {
      throw new Error("mpack: expected bin marker, got 0x" + b.toString(16));
    }
    const out = new Uint8Array(<i32>n);
    if (n > 0) {
      memory.copy(changetype<usize>(out.buffer) + out.byteOffset, this.regionPtr(<i32>n), <i32>n);
    }
    this.pos += <i32>n;
    return out;
  }

  readArrayHeader(): u32 {
    const b = this.take();
    if ((b & 0xf0) == 0x90) return b & 0x0f;
    if (b == 0xdc) return <u32>this.takeBe16();
    if (b == 0xdd) return <u32>this.takeBe32();
    throw new Error("mpack: expected array marker, got 0x" + b.toString(16));
  }

  readMapHeader(): u32 {
    const b = this.take();
    if ((b & 0xf0) == 0x80) return b & 0x0f;
    if (b == 0xde) return <u32>this.takeBe16();
    if (b == 0xdf) return <u32>this.takeBe32();
    throw new Error("mpack: expected map marker, got 0x" + b.toString(16));
  }

  // Reads a record/request map key. Integer keys of any width are returned
  // (negative keys collapse to 0); any non-integer key is consumed and also
  // reported as 0. Field numbers are 1-based, so 0 tells the caller to skip
  // the associated value — matching the host's lenient key handling.
  readMapKey(): u64 {
    if (this.pos >= this.bytes.length) {
      throw new Error("mpack: unexpected end of input");
    }
    const b = this.bytes[this.pos];
    const isInt = b <= 0x7f || b >= 0xe0 || (b >= 0xcc && b <= 0xd3);
    if (!isInt) {
      this.skipValue();
      return 0;
    }
    const v = this.readI64();
    return v >= 0 ? <u64>v : 0;
  }

  // Consumes one MessagePack value of any shape and discards it (used for
  // unknown record/request map keys).
  skipValue(): void {
    const b = this.take();
    if (b <= 0x7f || b >= 0xe0) return; // fixint
    if (b == 0xc0 || b == 0xc2 || b == 0xc3) return; // nil / bool
    if (b == 0xcc || b == 0xd0) {
      this.skipBytes(1);
      return;
    }
    if (b == 0xcd || b == 0xd1) {
      this.skipBytes(2);
      return;
    }
    if (b == 0xca || b == 0xce || b == 0xd2) {
      this.skipBytes(4);
      return;
    }
    if (b == 0xcb || b == 0xcf || b == 0xd3) {
      this.skipBytes(8);
      return;
    }
    if ((b & 0xe0) == 0xa0) {
      this.skipBytes(b & 0x1f);
      return;
    }
    if (b == 0xd9 || b == 0xc4) {
      this.skipBytes(this.take());
      return;
    }
    if (b == 0xda || b == 0xc5) {
      this.skipBytes(<i32>this.takeBe16());
      return;
    }
    if (b == 0xdb || b == 0xc6) {
      this.skipBytes(<i32>this.takeBe32());
      return;
    }
    if ((b & 0xf0) == 0x90) {
      this.skipItems(b & 0x0f);
      return;
    }
    if ((b & 0xf0) == 0x80) {
      this.skipItems(<u32>(b & 0x0f) * 2);
      return;
    }
    if (b == 0xdc) {
      this.skipItems(<u32>this.takeBe16());
      return;
    }
    if (b == 0xdd) {
      this.skipItems(<u32>this.takeBe32());
      return;
    }
    if (b == 0xde) {
      this.skipItems(<u32>this.takeBe16() * 2);
      return;
    }
    if (b == 0xdf) {
      this.skipItems(<u32>this.takeBe32() * 2);
      return;
    }
    throw new Error("mpack: cannot skip marker 0x" + b.toString(16));
  }

  isDone(): bool {
    return this.pos == this.bytes.length;
  }

  private skipItems(n: u32): void {
    for (let i: u32 = 0; i < n; i++) {
      this.skipValue();
    }
  }

  private skipBytes(n: i32): void {
    if (n < 0 || this.pos + n > this.bytes.length) {
      throw new Error("mpack: unexpected end of input");
    }
    this.pos += n;
  }

  private take(): u8 {
    if (this.pos >= this.bytes.length) {
      throw new Error("mpack: unexpected end of input");
    }
    const b = this.bytes[this.pos];
    this.pos += 1;
    return b;
  }

  private takeBe16(): u64 {
    return (<u64>this.take() << 8) | this.take();
  }

  private takeBe32(): u64 {
    return (<u64>this.take() << 24) | (<u64>this.take() << 16) | (<u64>this.take() << 8) | this.take();
  }

  private takeBe64(): u64 {
    return (this.takeBe32() << 32) | this.takeBe32();
  }

  // Pointer to a region of `n` bytes at the current position; bounds-checked.
  private regionPtr(n: i32): usize {
    if (n < 0 || this.pos + n > this.bytes.length) {
      throw new Error("mpack: unexpected end of input");
    }
    return changetype<usize>(this.bytes.buffer) + this.bytes.byteOffset + this.pos;
  }
}
