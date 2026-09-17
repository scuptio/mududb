// Runs the shared MessagePack primitive corpus tests against the
// hand-written canonical encoder/decoder in the codec package: for every
// vector of the cross-language corpus
// crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin (+ the
// mp_primitives_v1.json sidecar, generated from rmp_serde 1.3.1) the
// expected value is rebuilt from the sidecar (including the
// str_fill/bin_fill/array_fill/combo patterns), then checked that
//   - WriteValue encodes it to exactly the golden segment bytes (pins the
//     canonical encoder), and
//   - ReadValue decodes the segment back to the same value.
package corpus

import (
	"strconv"
	"strings"
	"testing"

	"github.com/ybbh/mududb_p/bindings/go/codec"
)

// f32Truncate truncates a float64 to single precision; e.g. 0.1 ->
// 0.10000000149011612. This is the expected behavior for f32 vectors.
func f32Truncate(v float64) float64 {
	return float64(float32(v))
}

// patternBin implements the sidecar `bin_fill` rule: byte i = i mod 251.
func patternBin(n int) []byte {
	out := make([]byte, n)
	for i := range out {
		out[i] = byte(i % 251)
	}
	return out
}

// expectedValue rebuilds the expected value from a sidecar vector entry.
func expectedValue(tb testing.TB, vector map[string]any) any {
	tb.Helper()
	kind := jsonString(vector, "kind")
	switch kind {
	case "u64":
		v, err := strconv.ParseUint(jsonString(vector, "value"), 10, 64)
		if err != nil {
			tb.Fatalf("u64 vector: %v", err)
		}
		return v
	case "i64":
		v, err := strconv.ParseInt(jsonString(vector, "value"), 10, 64)
		if err != nil {
			tb.Fatalf("i64 vector: %v", err)
		}
		return v
	case "f32":
		v, err := strconv.ParseFloat(jsonString(vector, "value"), 64)
		if err != nil {
			tb.Fatalf("f32 vector: %v", err)
		}
		return codec.F32(f32Truncate(v))
	case "f64":
		v, err := strconv.ParseFloat(jsonString(vector, "value"), 64)
		if err != nil {
			tb.Fatalf("f64 vector: %v", err)
		}
		return v
	case "nil":
		return nil
	case "bool":
		return vector["value"].(bool)
	case "str":
		if s, ok := vector["value"]; ok {
			return s.(string)
		}
		return strings.Repeat("a", int(jsonUint(vector, "len")))
	case "bin":
		return patternBin(int(jsonUint(vector, "len")))
	case "array":
		n := int(jsonUint(vector, "len"))
		out := make([]any, n)
		for i := 0; i < n; i++ {
			out[i] = uint64(i)
		}
		return out
	case "combo":
		return []any{uint64(42), "hi", []byte{0xde, 0xad, 0xbe, 0xef}}
	default:
		tb.Fatalf("unknown vector kind %s", kind)
		return nil
	}
}

// wireEqual compares two decoded wire values semantically: every integer
// width compares by value (a canonical non-negative i64 decodes as uint64),
// floats numerically, slices element-wise.
func wireEqual(a, b any) bool {
	if aNeg, aMag, ok := wireInt(a); ok {
		bNeg, bMag, ok := wireInt(b)
		return ok && aNeg == bNeg && aMag == bMag
	}
	switch av := a.(type) {
	case nil:
		return b == nil
	case bool:
		bv, ok := b.(bool)
		return ok && av == bv
	case string:
		bv, ok := b.(string)
		return ok && av == bv
	case float64:
		bv, ok := b.(float64)
		return ok && av == bv
	case []byte:
		bv, ok := b.([]byte)
		if !ok || len(av) != len(bv) {
			return false
		}
		for i := range av {
			if av[i] != bv[i] {
				return false
			}
		}
		return true
	case []any:
		bv, ok := b.([]any)
		if !ok || len(av) != len(bv) {
			return false
		}
		for i := range av {
			if !wireEqual(av[i], bv[i]) {
				return false
			}
		}
		return true
	default:
		return false
	}
}

// wireInt normalizes any integer-family value to (negative, magnitude) so
// every width — including the full u64 range — compares by value.
func wireInt(v any) (bool, uint64, bool) {
	switch t := v.(type) {
	case uint8:
		return false, uint64(t), true
	case uint16:
		return false, uint64(t), true
	case uint32:
		return false, uint64(t), true
	case uint64:
		return false, t, true
	case int8:
		return signedMagnitude(int64(t))
	case int16:
		return signedMagnitude(int64(t))
	case int32:
		return signedMagnitude(int64(t))
	case int64:
		return signedMagnitude(t)
	default:
		return false, 0, false
	}
}

func signedMagnitude(t int64) (bool, uint64, bool) {
	if t < 0 {
		// -(t + 1) + 1 keeps the min-int64 magnitude exact
		return true, uint64(-(t + 1)) + 1, true
	}
	return false, uint64(t), true
}

func vectorLabel(vector map[string]any) string {
	label := "#" + strconv.FormatUint(jsonUint(vector, "index"), 10) + " " + jsonString(vector, "kind")
	if v, ok := vector["value"]; ok {
		label += " " + strings.TrimSpace(strconv.Quote(strings.TrimSpace(jsonStringish(v))))
	}
	if _, ok := vector["len"]; ok {
		label += " len " + strconv.FormatUint(jsonUint(vector, "len"), 10)
	}
	return label
}

func jsonStringish(v any) string {
	switch t := v.(type) {
	case string:
		return t
	case bool:
		return strconv.FormatBool(t)
	case float64:
		return strconv.FormatFloat(t, 'g', -1, 64)
	default:
		return ""
	}
}

func TestMpPrimitivesCorpus(t *testing.T) {
	fixtureDir := findFixtureDir(t)
	segments, sidecar := loadFixture(t, fixtureDir, "mp_primitives_v1")
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
		label := vectorLabel(vector)
		segment := segments[int(jsonUint(vector, "index"))]
		value := expectedValue(t, vector)

		writer := codec.MpackWriter{}
		if err := writer.WriteValue(value); err != nil {
			t.Errorf("%s: encode: %v", label, err)
			continue
		}
		encoded := writer.Bytes()
		checks++
		checkBytes(t, label, "encode", encoded, segment)

		reader := codec.NewMpackReader(segment)
		decoded, err := reader.ReadValue()
		if err != nil {
			t.Errorf("%s: decode: %v", label, err)
			continue
		}
		checks++
		// f32 decodes as float64 (the reader is lenient across the float
		// markers); everything else compares through the value model
		expected := value
		if f32, ok := value.(codec.F32); ok {
			expected = float64(f32)
		}
		if !wireEqual(decoded, expected) {
			t.Errorf("%s: decode mismatch: %v (%T) vs %v (%T)", label, decoded, decoded, expected, expected)
		}
		checks++
		if !reader.IsDone() {
			t.Errorf("%s: trailing bytes after decode", label)
		}
	}
	t.Logf("%d checks passed across the mp_primitives_v1 corpus", checks)
}
