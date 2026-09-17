// MSSP v1 frame codec: the 16-byte header plus a single MessagePack body.
//
// Frame layout (all header fields big-endian):
// `magic "MSSP" | version 1 | flags 0 | message_kind`, then one MessagePack
// body. Request bodies are integer-keyed MessagePack maps of the
// WIT-declared arguments (1-based parameter numbers); result bodies are
// `[ok_tag, payload]` pairs with 0 = ok and 1 = err; unit results use the
// `[0, 0]` placeholder form.
//
// The message-kind validation (0 and values unknown to the generated
// MessageKind set are rejected) lives one layer up in the generated code;
// these helpers are deliberately shape-only.
package codec

import (
	"encoding/binary"
	"errors"
	"fmt"
)

// HeaderLen is the length in bytes of the fixed syscall payload header.
const HeaderLen = 16

// MSSPMagic is the fixed frame magic ("MSSP").
const MSSPMagic uint32 = 0x4D535350

// MSSPVersion is the supported payload version.
const MSSPVersion uint32 = 1

// EncodeHeader returns the fixed 16-byte header for a message kind.
func EncodeHeader(kind uint32) []byte {
	frame := make([]byte, HeaderLen)
	binary.BigEndian.PutUint32(frame[0:4], MSSPMagic)
	binary.BigEndian.PutUint32(frame[4:8], MSSPVersion)
	binary.BigEndian.PutUint32(frame[8:12], 0)
	binary.BigEndian.PutUint32(frame[12:16], kind)
	return frame
}

// DecodeHeader decodes and validates the fixed 16-byte header of frame,
// returning the raw message kind (membership in the generated MessageKind
// set is checked by the caller). It fails on any integrity violation.
func DecodeHeader(frame []byte) (uint32, error) {
	if len(frame) < HeaderLen {
		return 0, fmt.Errorf("MSSP header shorter than %d bytes (%d)", HeaderLen, len(frame))
	}
	if magic := binary.BigEndian.Uint32(frame[0:4]); magic != MSSPMagic {
		return 0, fmt.Errorf("MSSP bad magic 0x%08x, expected 0x%08x", magic, MSSPMagic)
	}
	if version := binary.BigEndian.Uint32(frame[4:8]); version != MSSPVersion {
		return 0, fmt.Errorf("MSSP unsupported payload version %d, supported range is [1, 1]", version)
	}
	if flags := binary.BigEndian.Uint32(frame[8:12]); flags != 0 {
		return 0, fmt.Errorf("MSSP nonzero header flags 0x%x", flags)
	}
	return binary.BigEndian.Uint32(frame[12:16]), nil
}

// EncodeFrame returns a complete frame: the 16-byte header followed by body.
func EncodeFrame(kind uint32, body []byte) []byte {
	frame := EncodeHeader(kind)
	return append(frame, body...)
}

// ExpectFrameBody validates a frame, checks it carries the expected message
// kind, and returns the body.
func ExpectFrameBody(frame []byte, expected uint32) ([]byte, error) {
	kind, err := DecodeHeader(frame)
	if err != nil {
		return nil, err
	}
	if kind != expected {
		return nil, fmt.Errorf("MSSP unexpected syscall message kind %d, expected %d", kind, expected)
	}
	return frame[HeaderLen:], nil
}

// EncodeRequestFrame encodes a request frame: the body is the integer-keyed
// MessagePack map of the WIT-declared arguments (1-based parameter numbers).
func EncodeRequestFrame(kind uint32, body map[uint64]any) ([]byte, error) {
	writer := MpackWriter{}
	if err := writer.WriteValue(body); err != nil {
		return nil, err
	}
	return EncodeFrame(kind, writer.Bytes()), nil
}

// DecodeRequestMap decodes a request frame into its argument map;
// non-integer keys are skipped (they cannot be WIT parameters) and trailing
// body bytes are rejected.
func DecodeRequestMap(expected uint32, frame []byte) (map[uint64]any, error) {
	body, err := ExpectFrameBody(frame, expected)
	if err != nil {
		return nil, err
	}
	value, err := readWholeValue(body)
	if err != nil {
		return nil, err
	}
	pairs, ok := value.(map[any]any)
	if !ok {
		return nil, fmt.Errorf("request body must be an integer-keyed map, found %T", value)
	}
	fields := make(map[uint64]any, len(pairs))
	for key, item := range pairs {
		number, isInt := MapKeyUint(key)
		if !isInt {
			continue
		}
		fields[number] = item
	}
	return fields, nil
}

// EncodeResultOkFrame encodes a result frame whose body is `[0, value]`.
func EncodeResultOkFrame(kind uint32, okWire any) ([]byte, error) {
	return encodeResultFrame(kind, []any{uint8(0), okWire})
}

// EncodeUnitResultOkFrame encodes a unit result frame whose body is `[0, 0]`.
func EncodeUnitResultOkFrame(kind uint32) []byte {
	// the unit placeholder cannot fail to encode
	frame, _ := encodeResultFrame(kind, []any{uint8(0), uint8(0)})
	return frame
}

// EncodeResultErrFrame encodes a result frame whose body is `[1, error]`.
func EncodeResultErrFrame(kind uint32, errWire any) ([]byte, error) {
	return encodeResultFrame(kind, []any{uint8(1), errWire})
}

func encodeResultFrame(kind uint32, body []any) ([]byte, error) {
	writer := MpackWriter{}
	if err := writer.WriteValue(body); err != nil {
		return nil, err
	}
	return EncodeFrame(kind, writer.Bytes()), nil
}

// DecodeResultBody decodes a result frame body: it returns (isOk, payload)
// with the payload kept in raw wire-value form for the caller to convert
// (both the ok and the err arm). Trailing body bytes are rejected.
func DecodeResultBody(expected uint32, frame []byte) (bool, any, error) {
	body, err := ExpectFrameBody(frame, expected)
	if err != nil {
		return false, nil, err
	}
	value, err := readWholeValue(body)
	if err != nil {
		return false, nil, err
	}
	seq, ok := value.([]any)
	if !ok || len(seq) != 2 {
		return false, nil, fmt.Errorf("result body must be a 2-element [tag, payload] array, found %v", value)
	}
	tag, ok := MapKeyUint(seq[0])
	if !ok {
		return false, nil, fmt.Errorf("result body tag must be a non-negative integer, found %v", seq[0])
	}
	switch tag {
	case 0:
		return true, seq[1], nil
	case 1:
		return false, seq[1], nil
	default:
		return false, nil, fmt.Errorf("MSSP unknown result tag %d, expected 0 (ok) or 1 (err)", tag)
	}
}

// readWholeValue decodes exactly one MessagePack value and rejects trailing
// bytes.
func readWholeValue(body []byte) (any, error) {
	reader := NewMpackReader(body)
	value, err := reader.ReadValue()
	if err != nil {
		return nil, err
	}
	if !reader.IsDone() {
		return nil, errors.New("trailing bytes after syscall payload body")
	}
	return value, nil
}

// MapKeyUint converts a wire map key to its field number. Integer keys of
// any width are returned (negative keys collapse to not-ok); any
// non-integer key reports not-ok, so the caller skips the associated value
// — matching the host's lenient key handling.
func MapKeyUint(key any) (uint64, bool) {
	switch k := key.(type) {
	case uint8:
		return uint64(k), true
	case uint16:
		return uint64(k), true
	case uint32:
		return uint64(k), true
	case uint64:
		return k, true
	case uint:
		return uint64(k), true
	case int8:
		return signedKey(int64(k))
	case int16:
		return signedKey(int64(k))
	case int32:
		return signedKey(int64(k))
	case int64:
		return signedKey(k)
	case int:
		return signedKey(int64(k))
	default:
		return 0, false
	}
}

func signedKey(v int64) (uint64, bool) {
	if v < 0 {
		return 0, false
	}
	return uint64(v), true
}
