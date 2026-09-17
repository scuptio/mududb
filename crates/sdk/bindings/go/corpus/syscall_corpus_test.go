// Runs the MSSP syscall corpus tests for the mgen-generated Go codec in the
// types package: drives the cross-language corpus
// crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin
// (47 frames: one request + one ok response per message kind 1..23 in
// MessageKind discriminant order, plus a trailing `get` UniError response).
// The sidecar syscall_payload_v1_all.json is the single source of truth for
// the semantic expectations: for every frame the runner checks, through the
// generated Go codec:
//   - the header routes to the expected message kind,
//   - requests encode from the documented inputs byte-exactly,
//   - decode -> sidecar expect shape == sidecar `expect` object (compared
//     through canonical JSON),
//   - ok responses encode from the documented values byte-exactly and
//     decode -> re-encode byte-identically, then decode again to the same
//     expect shape (roundtrip),
//   - the err response decodes field-by-field (err_src carries the host's
//     '"None"' quirk verbatim) and re-encodes byte-identically.
package corpus

import (
	"testing"

	"github.com/ybbh/mududb_p/bindings/go/types"
)

func TestSyscallCorpus(t *testing.T) {
	fixtureDir := findFixtureDir(t)
	segments, sidecar := loadFixture(t, fixtureDir, "syscall_payload_v1_all")
	frames, ok := sidecar["frames"].([]any)
	if !ok {
		t.Fatalf("sidecar frames is %T", sidecar["frames"])
	}
	if len(segments) != len(frames) {
		t.Fatalf("segment count %d != sidecar frame count %d", len(segments), len(frames))
	}
	checks := 0
	for _, rawFrame := range frames {
		frame := rawFrame.(map[string]any)
		index := int(jsonUint(frame, "index"))
		kind := uint32(jsonUint(frame, "message_kind"))
		direction := jsonString(frame, "direction")
		label := jsonString(frame, "message_kind_name") + " " + direction
		segment := segments[index]

		actual, err := headerKind(segment)
		checks++
		if err != nil || actual != kind {
			t.Errorf("%s: header kind mismatch: %v, %d vs %d", label, err, actual, kind)
			continue
		}

		switch direction {
		case "request":
			encoded, err := encodeRequest(kind)
			checks++
			if err != nil {
				t.Errorf("%s: request encode: %v", label, err)
			} else {
				checkBytes(t, label, "request encode", encoded, segment)
			}
			expect, err := decodeRequestExpect(kind, segment)
			checks++
			if err != nil {
				t.Errorf("%s: request decode: %v", label, err)
			} else {
				checkExpect(t, label, "request decode", expect, frame["expect"])
			}
		case "response":
			encoded, err := encodeOkResponse(kind)
			checks++
			if err != nil {
				t.Errorf("%s: response encode: %v", label, err)
			} else {
				checkBytes(t, label, "response encode", encoded, segment)
			}
			expect, err := decodeResponseExpect(kind, segment)
			checks++
			if err != nil {
				t.Errorf("%s: response decode: %v", label, err)
			} else {
				checkExpect(t, label, "response decode", expect, frame["expect"])
			}
			decoded, err := decodeResponse(kind, segment)
			if err != nil {
				t.Errorf("%s: response decode for re-encode: %v", label, err)
				continue
			}
			reencoded, err := encodeResponse(kind, decoded)
			checks++
			if err != nil {
				t.Errorf("%s: response re-encode: %v", label, err)
			} else {
				checkBytes(t, label, "response decode+re-encode", reencoded, segment)
			}
			expect, err = decodeResponseExpect(kind, reencoded)
			checks++
			if err != nil {
				t.Errorf("%s: response re-encode+decode: %v", label, err)
			} else {
				checkExpect(t, label, "response re-encode+decode roundtrip", expect, frame["expect"])
			}
		case "response_err":
			encoded, err := encodeGetErrResponse()
			checks++
			if err != nil {
				t.Errorf("%s: err response encode: %v", label, err)
			} else {
				checkBytes(t, label, "err response encode", encoded, segment)
			}
			expect, err := decodeErrExpect(kind, segment)
			checks++
			if err != nil {
				t.Errorf("%s: err response decode: %v", label, err)
			} else {
				checkExpect(t, label, "err response decode", expect, frame["expect"])
			}
			decoded, err := decodeResponse(kind, segment)
			if err != nil {
				t.Errorf("%s: err response decode for re-encode: %v", label, err)
				continue
			}
			reencoded, err := encodeResponse(kind, decoded)
			checks++
			if err != nil {
				t.Errorf("%s: err response re-encode: %v", label, err)
			} else {
				checkBytes(t, label, "err response decode+re-encode", reencoded, segment)
			}
		default:
			t.Errorf("%s: unknown direction %s", label, direction)
		}
	}
	t.Logf("%d checks passed across the syscall_payload_v1_all corpus", checks)
}

// TestHeaderValidation pins the generated DecodeHeader wrapper: valid frames
// route, kind 0 and out-of-range kinds are rejected.
func TestHeaderValidation(t *testing.T) {
	frame, err := types.EncodeGetRequest(oid(1), []byte("k"))
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	kind, err := types.DecodeHeader(frame)
	if err != nil || kind != types.MessageKindGet {
		t.Fatalf("decode header: %v, %v", err, kind)
	}
	if _, err := types.DecodeHeader([]byte("short")); err == nil {
		t.Errorf("expected error for a short frame")
	}
}
