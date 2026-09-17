// Runs the MSSP lenient-decode vector tests for the mgen-generated Go
// codec: drives the hand-built NON-canonical frames in
// crates/db-kernel/testing/fixtures/golden/v1/lenient_decode_v1.bin
// (8 vectors: integer width widening, unsigned markers for signed values,
// wide negative ints, record map key reordering, unknown/skipped map keys,
// missing request parameters, missing record fields/variant cases).
// The vectors are not re-encodable by the canonical encoder, so no encode
// or re-encode comparison is done; for every vector the runner checks,
// through the generated Go codec:
//   - the header routes to the expected message kind,
//   - decode -> sidecar expect shape == sidecar `expect` object from
//     lenient_decode_v1.json, which is the single source of truth.
package corpus

import "testing"

func TestLenientDecodeCorpus(t *testing.T) {
	fixtureDir := findFixtureDir(t)
	segments, sidecar := loadFixture(t, fixtureDir, "lenient_decode_v1")
	vectors, ok := sidecar["vectors"].([]any)
	if !ok {
		t.Fatalf("sidecar vectors is %T", sidecar["vectors"])
	}
	if len(segments) != len(vectors) {
		t.Fatalf("segment count %d != sidecar vector count %d", len(segments), len(vectors))
	}
	checks := 0
	for _, rawVector := range vectors {
		vector := rawVector.(map[string]any)
		kind := uint32(jsonUint(vector, "message_kind"))
		direction := jsonString(vector, "direction")
		label := jsonString(vector, "kind") + " (" + jsonString(vector, "message_kind_name") + " " + direction + ")"
		segment := segments[int(jsonUint(vector, "index"))]

		actual, err := headerKind(segment)
		checks++
		if err != nil || actual != kind {
			t.Errorf("%s: header kind mismatch: %v, %d vs %d", label, err, actual, kind)
			continue
		}

		var expect any
		switch direction {
		case "request":
			expect, err = decodeRequestExpect(kind, segment)
		case "response":
			expect, err = decodeResponseExpect(kind, segment)
		default:
			t.Errorf("%s: unknown direction %s", label, direction)
			continue
		}
		checks++
		if err != nil {
			t.Errorf("%s: decode: %v (%s)", label, err, jsonString(vector, "note"))
		} else {
			checkExpect(t, label, "decode", expect, vector["expect"])
		}
	}
	t.Logf("%d checks passed across the lenient_decode_v1 corpus", checks)
}
