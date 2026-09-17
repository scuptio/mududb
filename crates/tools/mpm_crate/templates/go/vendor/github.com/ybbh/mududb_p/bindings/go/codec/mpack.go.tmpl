// Package codec is the hand-written MessagePack and MSSP frame runtime used
// by the mgen-generated Go codecs in the sibling `types` package.
//
// Encoding is byte-exact with rmp-serde 1.3.x (rmp_serde::to_vec):
//
//   - Integers use minimal-width encoding BY VALUE, independent of the Go
//     type. Non-negative values use the unsigned marker chain (fixint /
//     0xCC u8 / 0xCD u16 / 0xCE u32 / 0xCF u64); negative values use
//     negfixint / 0xD0 i8 / 0xD1 i16 / 0xD2 i32 / 0xD3 i64.
//   - Strings use fixstr / str8 (0xD9) / str16 (0xDA) / str32 (0xDB);
//     rmp_serde DOES emit str8 for lengths 32..=255.
//   - Binary uses bin8 (0xC4) / bin16 (0xC5) / bin32 (0xC6).
//   - Arrays use fixarray / array16 (0xDC) / array32 (0xDD). Maps use
//     fixmap / map16 (0xDE) / map32 (0xDF); the wire maps of this package
//     are integer-keyed (record field and request parameter numbers), so
//     map[uint64]any keys are written in ASCENDING order — the field
//     declaration order the canonical host encoder uses (Go map iteration
//     order is random and would not be byte-stable).
//   - float64 values encode as f64 (0xCB). The F32 marker type (and plain
//     float32 values) encode as f32 (0xCA). Decoding floats is lenient
//     across the 0xCA/0xCB markers, matching rmp_serde::from_slice.
//
// The value model: WriteValue/ReadValue convert between the wire and native
// Go values — nil, bool, uint64 (any unsigned marker), int64 (any signed
// marker), float64 (0xCA/0xCB), string, []byte (bin), []any (array) and
// map[any]any (map, read side). Reads are lenient (every integer width is
// accepted everywhere), writes are canonical.
//
// The code is TinyGo-compatible: no unsafe, no generics, no reflection.
package codec

import (
	"encoding/binary"
	"errors"
	"fmt"
	"math"
)

// F32 marks a float64 value for f32 (0xCA + big-endian IEEE-754 single)
// encoding; a plain float64 encodes as f64 (0xCB). Construct with an
// already f32-truncated value when the exact wire value matters; the writer
// truncates to single precision on encode regardless.
type F32 float64

// ---- writer ----

// MpackWriter is a canonical (rmp_serde-compatible) MessagePack writer.
type MpackWriter struct {
	buf []byte
}

// Bytes returns the encoded bytes accumulated so far.
func (w *MpackWriter) Bytes() []byte { return w.buf }

func (w *MpackWriter) put(b byte) { w.buf = append(w.buf, b) }

func (w *MpackWriter) putBytes(bs []byte) { w.buf = append(w.buf, bs...) }

// WriteNil appends the nil marker (0xC0).
func (w *MpackWriter) WriteNil() { w.put(0xc0) }

// WriteBool appends a bool.
func (w *MpackWriter) WriteBool(v bool) {
	if v {
		w.put(0xc3)
	} else {
		w.put(0xc2)
	}
}

// WriteU64 appends a non-negative integer in minimal-width unsigned form.
func (w *MpackWriter) WriteU64(v uint64) {
	switch {
	case v <= 0x7f:
		w.put(byte(v))
	case v <= math.MaxUint8:
		w.put(0xcc)
		w.put(byte(v))
	case v <= math.MaxUint16:
		w.put(0xcd)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(v))
		w.putBytes(tmp[:])
	case v <= math.MaxUint32:
		w.put(0xce)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], uint32(v))
		w.putBytes(tmp[:])
	default:
		w.put(0xcf)
		var tmp [8]byte
		binary.BigEndian.PutUint64(tmp[:], v)
		w.putBytes(tmp[:])
	}
}

// WriteI64 appends a signed integer; non-negative values go through the
// unsigned chain (rmp_serde encodes by value, not by declared type).
func (w *MpackWriter) WriteI64(v int64) {
	if v >= 0 {
		w.WriteU64(uint64(v))
		return
	}
	switch {
	case v >= -32:
		w.put(byte(0xe0 | byte(v+32)))
	case v >= math.MinInt8:
		w.put(0xd0)
		w.put(byte(int8(v)))
	case v >= math.MinInt16:
		w.put(0xd1)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(int16(v)))
		w.putBytes(tmp[:])
	case v >= math.MinInt32:
		w.put(0xd2)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], uint32(int32(v)))
		w.putBytes(tmp[:])
	default:
		w.put(0xd3)
		var tmp [8]byte
		binary.BigEndian.PutUint64(tmp[:], uint64(v))
		w.putBytes(tmp[:])
	}
}

// WriteF32 appends a float in f32 form (0xCA + big-endian IEEE-754 single).
func (w *MpackWriter) WriteF32(v float64) {
	w.put(0xca)
	var tmp [4]byte
	binary.BigEndian.PutUint32(tmp[:], math.Float32bits(float32(v)))
	w.putBytes(tmp[:])
}

// WriteF64 appends a float in f64 form (0xCB + big-endian IEEE-754 double).
func (w *MpackWriter) WriteF64(v float64) {
	w.put(0xcb)
	var tmp [8]byte
	binary.BigEndian.PutUint64(tmp[:], math.Float64bits(v))
	w.putBytes(tmp[:])
}

// WriteStr appends a string (fixstr / str8 / str16 / str32).
func (w *MpackWriter) WriteStr(v string) {
	n := len(v)
	switch {
	case n < 32:
		w.put(byte(0xa0 | byte(n)))
	case n <= math.MaxUint8:
		w.put(0xd9)
		w.put(byte(n))
	case n <= math.MaxUint16:
		w.put(0xda)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(n))
		w.putBytes(tmp[:])
	default:
		w.put(0xdb)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], uint32(n))
		w.putBytes(tmp[:])
	}
	w.putBytes([]byte(v))
}

// WriteBin appends a binary blob (bin8 / bin16 / bin32).
func (w *MpackWriter) WriteBin(v []byte) {
	n := len(v)
	switch {
	case n <= math.MaxUint8:
		w.put(0xc4)
		w.put(byte(n))
	case n <= math.MaxUint16:
		w.put(0xc5)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(n))
		w.putBytes(tmp[:])
	default:
		w.put(0xc6)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], uint32(n))
		w.putBytes(tmp[:])
	}
	w.putBytes(v)
}

// WriteArrayHeader appends an array header for n elements.
func (w *MpackWriter) WriteArrayHeader(n uint32) {
	switch {
	case n < 16:
		w.put(byte(0x90 | n))
	case n <= math.MaxUint16:
		w.put(0xdc)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(n))
		w.putBytes(tmp[:])
	default:
		w.put(0xdd)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], n)
		w.putBytes(tmp[:])
	}
}

// WriteMapHeader appends a map header for n pairs.
func (w *MpackWriter) WriteMapHeader(n uint32) {
	switch {
	case n < 16:
		w.put(byte(0x80 | n))
	case n <= math.MaxUint16:
		w.put(0xde)
		var tmp [2]byte
		binary.BigEndian.PutUint16(tmp[:], uint16(n))
		w.putBytes(tmp[:])
	default:
		w.put(0xdf)
		var tmp [4]byte
		binary.BigEndian.PutUint32(tmp[:], n)
		w.putBytes(tmp[:])
	}
}

// WriteValue recursively writes a native Go value in canonical form. The
// supported shapes are the wire value model: nil, bool, every integer width,
// F32/float32/float64, string, []byte, []any and map[uint64]any (keys
// ascending; see the package doc). Anything else is an error.
func (w *MpackWriter) WriteValue(v any) error {
	switch t := v.(type) {
	case nil:
		w.WriteNil()
	case bool:
		w.WriteBool(t)
	case uint8:
		w.WriteU64(uint64(t))
	case uint16:
		w.WriteU64(uint64(t))
	case uint32:
		w.WriteU64(uint64(t))
	case uint64:
		w.WriteU64(t)
	case uint:
		w.WriteU64(uint64(t))
	case int8:
		w.WriteI64(int64(t))
	case int16:
		w.WriteI64(int64(t))
	case int32:
		w.WriteI64(int64(t))
	case int64:
		w.WriteI64(t)
	case int:
		w.WriteI64(int64(t))
	case F32:
		w.WriteF32(float64(t))
	case float32:
		w.WriteF32(float64(t))
	case float64:
		w.WriteF64(t)
	case string:
		w.WriteStr(t)
	case []byte:
		w.WriteBin(t)
	case []any:
		w.WriteArrayHeader(uint32(len(t)))
		for _, item := range t {
			if err := w.WriteValue(item); err != nil {
				return err
			}
		}
	case map[uint64]any:
		w.WriteMapHeader(uint32(len(t)))
		for _, key := range sortedKeys(t) {
			w.WriteU64(key)
			if err := w.WriteValue(t[key]); err != nil {
				return err
			}
		}
	default:
		return fmt.Errorf("mpack: unsupported type for MessagePack: %T", v)
	}
	return nil
}

// sortedKeys returns the map keys in ascending order. The wire maps of this
// package are integer-keyed record field / request parameter maps whose keys
// are the 1-based declaration numbers, so ascending order matches the
// canonical host encoder. Insertion sort keeps this free of the `sort`
// import (TinyGo-friendly) and is linear for the already-ordered bodies the
// generated encoders build.
func sortedKeys(m map[uint64]any) []uint64 {
	keys := make([]uint64, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	for i := 1; i < len(keys); i++ {
		for j := i; j > 0 && keys[j] < keys[j-1]; j-- {
			keys[j], keys[j-1] = keys[j-1], keys[j]
		}
	}
	return keys
}

// ---- reader ----

// MpackReader is a lenient MessagePack reader over a byte buffer. It carries
// a sticky error: once a read fails, every later read is a no-op returning
// zero values, so call sites check the error once at the end of a decode
// pass instead of after every field.
type MpackReader struct {
	data []byte
	pos  int
	err  error // sticky: set on the first malformed read
}

var errMpUnexpectedEnd = errors.New("mpack: unexpected end of input")

// NewMpackReader creates a reader over data.
func NewMpackReader(data []byte) *MpackReader {
	return &MpackReader{data: data}
}

// IsDone reports whether every input byte has been consumed.
func (r *MpackReader) IsDone() bool {
	return r.pos == len(r.data)
}

// Err returns the sticky error, if any.
func (r *MpackReader) Err() error { return r.err }

func (r *MpackReader) fail(format string, args ...any) {
	if r.err == nil {
		r.err = fmt.Errorf(format, args...)
	}
}

func (r *MpackReader) take() byte {
	if r.err != nil {
		return 0
	}
	if r.pos >= len(r.data) {
		r.err = errMpUnexpectedEnd
		return 0
	}
	b := r.data[r.pos]
	r.pos++
	return b
}

func (r *MpackReader) takeN(n int) []byte {
	if r.err != nil {
		return nil
	}
	if r.pos+n > len(r.data) {
		r.err = errMpUnexpectedEnd
		return nil
	}
	s := r.data[r.pos : r.pos+n]
	r.pos += n
	return s
}

func (r *MpackReader) takeBE16() uint16 {
	s := r.takeN(2)
	if r.err != nil {
		return 0
	}
	return binary.BigEndian.Uint16(s)
}

func (r *MpackReader) takeBE32() uint32 {
	s := r.takeN(4)
	if r.err != nil {
		return 0
	}
	return binary.BigEndian.Uint32(s)
}

func (r *MpackReader) takeBE64() uint64 {
	s := r.takeN(8)
	if r.err != nil {
		return 0
	}
	return binary.BigEndian.Uint64(s)
}

// ReadValue reads one value of any shape into the wire value model: any
// unsigned marker -> uint64, any signed marker -> int64, 0xCA/0xCB ->
// float64, str -> string, bin -> []byte, array -> []any, map -> map[any]any.
func (r *MpackReader) ReadValue() (any, error) {
	v := r.readValue()
	if r.err != nil {
		return nil, r.err
	}
	return v, nil
}

func (r *MpackReader) readValue() any {
	b := r.take()
	if r.err != nil {
		return nil
	}
	switch {
	case b <= 0x7f:
		return uint64(b)
	case b >= 0xe0:
		return int64(int8(b))
	case (b & 0xf0) == 0x80:
		return r.readMap(uint32(b & 0x0f))
	case (b & 0xf0) == 0x90:
		return r.readArray(uint32(b & 0x0f))
	case (b & 0xe0) == 0xa0:
		return r.takeString(uint32(b & 0x1f))
	}
	switch b {
	case 0xc0:
		return nil
	case 0xc2:
		return false
	case 0xc3:
		return true
	case 0xc4:
		return r.takeBin(uint32(r.take()))
	case 0xc5:
		return r.takeBin(uint32(r.takeBE16()))
	case 0xc6:
		return r.takeBin(r.takeBE32())
	case 0xca:
		return float64(math.Float32frombits(r.takeBE32()))
	case 0xcb:
		return math.Float64frombits(r.takeBE64())
	case 0xcc:
		return uint64(r.take())
	case 0xcd:
		return uint64(r.takeBE16())
	case 0xce:
		return uint64(r.takeBE32())
	case 0xcf:
		return r.takeBE64()
	case 0xd0:
		return int64(int8(r.take()))
	case 0xd1:
		return int64(int16(r.takeBE16()))
	case 0xd2:
		return int64(int32(r.takeBE32()))
	case 0xd3:
		return int64(r.takeBE64())
	case 0xd9:
		return r.takeString(uint32(r.take()))
	case 0xda:
		return r.takeString(uint32(r.takeBE16()))
	case 0xdb:
		return r.takeString(r.takeBE32())
	case 0xdc:
		return r.readArray(uint32(r.takeBE16()))
	case 0xdd:
		return r.readArray(r.takeBE32())
	case 0xde:
		return r.readMap(uint32(r.takeBE16()))
	case 0xdf:
		return r.readMap(r.takeBE32())
	default:
		r.fail("mpack: unknown marker 0x%02x", b)
		return nil
	}
}

func (r *MpackReader) readArray(n uint32) []any {
	out := make([]any, 0, n)
	for i := uint32(0); i < n; i++ {
		out = append(out, r.readValue())
		if r.err != nil {
			return nil
		}
	}
	return out
}

func (r *MpackReader) readMap(n uint32) map[any]any {
	out := make(map[any]any, n)
	for i := uint32(0); i < n; i++ {
		key := r.readValue()
		if r.err != nil {
			return nil
		}
		value := r.readValue()
		if r.err != nil {
			return nil
		}
		out[key] = value
	}
	return out
}

func (r *MpackReader) takeString(n uint32) string {
	if r.err != nil {
		return ""
	}
	return string(r.takeN(int(n)))
}

func (r *MpackReader) takeBin(n uint32) []byte {
	if r.err != nil {
		return nil
	}
	return r.takeN(int(n))
}
