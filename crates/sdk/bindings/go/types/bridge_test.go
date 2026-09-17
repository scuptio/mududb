// Tests for the record bridge (bridge.go) and the wire-helper leniency the
// procedure byte pipe relies on (bools crossing as i32 0/1, value-model map
// forms).
package types

import (
	"reflect"
	"testing"

	"github.com/ybbh/mududb_p/bindings/go/codec"
)

func scalarString(s string) UniDataValue {
	return UniDataValueScalar{Inner: UniScalarValueString{Inner: s}}
}

func scalarI32(n int32) UniDataValue {
	return UniDataValueScalar{Inner: UniScalarValueI32{Inner: n}}
}

func scalarI64(n int64) UniDataValue {
	return UniDataValueScalar{Inner: UniScalarValueI64{Inner: n}}
}

func TestRecordFieldValuesUnwrapsPositionalFields(t *testing.T) {
	envelope := UniDataValueRecord{Inner: []UniDataValueField{
		{FieldName: "", FieldValue: scalarString("ada")},
		{FieldName: "", FieldValue: scalarI32(7)},
		{FieldName: "", FieldValue: scalarI32(1)}, // a bool crossing as i32
		{FieldName: "", FieldValue: UniDataValueScalar{Inner: UniScalarValueF64{Inner: 2.5}}},
		{FieldName: "", FieldValue: UniDataValueScalar{Inner: UniScalarValueBlob{Inner: []byte{1, 2}}}},
		{FieldName: "", FieldValue: UniDataValueScalar{Inner: UniScalarValueNull{}}},
	}}
	fields, err := RecordFieldValues(envelope)
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	expected := map[uint64]any{
		1: "ada",
		2: int64(7),
		3: int64(1),
		4: 2.5,
		5: []byte{1, 2},
		6: nil,
	}
	if !reflect.DeepEqual(fields, expected) {
		t.Fatalf("fields = %#v, expected %#v", fields, expected)
	}
}

func TestRecordFieldValuesRecursesIntoRecordsAndArrays(t *testing.T) {
	envelope := UniDataValueRecord{Inner: []UniDataValueField{
		{FieldValue: UniDataValueRecord{Inner: []UniDataValueField{
			{FieldValue: scalarString("sh")},
			{FieldValue: scalarString("200")},
		}}},
		{FieldValue: UniDataValueArray{Inner: []UniDataValue{
			scalarString("a"),
			scalarString("b"),
		}}},
	}}
	fields, err := RecordFieldValues(envelope)
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	expected := map[uint64]any{
		1: map[uint64]any{1: "sh", 2: "200"},
		2: []any{"a", "b"},
	}
	if !reflect.DeepEqual(fields, expected) {
		t.Fatalf("fields = %#v, expected %#v", fields, expected)
	}
}

func TestRecordFieldValuesRejectsNonRecord(t *testing.T) {
	for _, uv := range []UniDataValue{
		scalarString("x"),
		UniDataValueArray{Inner: nil},
		UniDataValueBinary{Inner: []byte{1}},
	} {
		if _, err := RecordFieldValues(uv); err == nil {
			t.Fatalf("expected an error for %T", uv)
		}
	}
}

func TestRecordFieldValuesEmptyRecord(t *testing.T) {
	fields, err := RecordFieldValues(UniDataValueRecord{Inner: nil})
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	if fields == nil || len(fields) != 0 {
		t.Fatalf("expected an empty non-nil map, got %#v", fields)
	}
}

func TestRecordFromFieldValuesMapKeepsDeclarationOrder(t *testing.T) {
	envelope, err := RecordFromFieldValues(map[uint64]any{
		1: "ada",
		2: uint32(7),
		3: true,
		4: []any{"a", "b"},
		5: map[uint64]any{1: "sh", 2: "200"},
	})
	if err != nil {
		t.Fatalf("RecordFromFieldValues: %v", err)
	}
	record, ok := envelope.(UniDataValueRecord)
	if !ok {
		t.Fatalf("expected a record envelope, got %T", envelope)
	}
	if len(record.Inner) != 5 {
		t.Fatalf("expected 5 fields, got %d", len(record.Inner))
	}
	expected := []UniDataValue{
		scalarString("ada"),
		scalarI32(7),
		scalarI32(1), // the bool crossed as i32
		UniDataValueArray{Inner: []UniDataValue{scalarString("a"), scalarString("b")}},
		UniDataValueRecord{Inner: []UniDataValueField{
			{FieldName: "", FieldValue: scalarString("sh")},
			{FieldName: "", FieldValue: scalarString("200")},
		}},
	}
	for i, field := range record.Inner {
		if field.FieldName != "" {
			t.Fatalf("field %d: expected an empty name, got %q", i+1, field.FieldName)
		}
		if !reflect.DeepEqual(field.FieldValue, expected[i]) {
			t.Fatalf("field %d = %#v, expected %#v", i+1, field.FieldValue, expected[i])
		}
	}
}

func TestRecordFromFieldValuesFillsOmittedSlotsWithNull(t *testing.T) {
	// an omitted middle option field (an XxxToValue map skips nil options)
	// keeps its declaration slot as the Null scalar
	envelope, err := RecordFromFieldValues(map[uint64]any{
		1: "ada",
		3: "lo",
	})
	if err != nil {
		t.Fatalf("RecordFromFieldValues: %v", err)
	}
	record := envelope.(UniDataValueRecord)
	if len(record.Inner) != 3 {
		t.Fatalf("expected 3 slots, got %d", len(record.Inner))
	}
	if !reflect.DeepEqual(record.Inner[1].FieldValue, UniDataValueScalar{Inner: UniScalarValueNull{}}) {
		t.Fatalf("slot 2 = %#v, expected the Null scalar", record.Inner[1].FieldValue)
	}
}

func TestRecordFromFieldValuesSliceForm(t *testing.T) {
	envelope, err := RecordFromFieldValues([]any{"ada", int64(7), nil})
	if err != nil {
		t.Fatalf("RecordFromFieldValues: %v", err)
	}
	record := envelope.(UniDataValueRecord)
	if len(record.Inner) != 3 {
		t.Fatalf("expected 3 fields, got %d", len(record.Inner))
	}
	if !reflect.DeepEqual(record.Inner[2].FieldValue, UniDataValueScalar{Inner: UniScalarValueNull{}}) {
		t.Fatalf("field 3 = %#v, expected the Null scalar", record.Inner[2].FieldValue)
	}
}

func TestRecordFromFieldValuesEmptyRecord(t *testing.T) {
	for _, input := range []any{map[uint64]any{}, []any{}} {
		envelope, err := RecordFromFieldValues(input)
		if err != nil {
			t.Fatalf("RecordFromFieldValues: %v", err)
		}
		record := envelope.(UniDataValueRecord)
		if len(record.Inner) != 0 {
			t.Fatalf("expected 0 fields, got %d", len(record.Inner))
		}
	}
}

func TestRecordFromFieldValuesRejectsBadInput(t *testing.T) {
	if _, err := RecordFromFieldValues("not-a-record"); err == nil {
		t.Fatal("expected an error for a string input")
	}
}

func TestRecordFromFieldValuesRejectsOutOfVocabularyIntegers(t *testing.T) {
	// u64 above the host's i64 vocabulary and u32 above its i32 vocabulary
	// are hard errors, not silent wraps
	if _, err := RecordFromFieldValues([]any{uint64(1) << 63}); err == nil {
		t.Fatal("expected an overflow error for u64")
	}
	if _, err := RecordFromFieldValues([]any{uint32(1) << 31}); err == nil {
		t.Fatal("expected an overflow error for u32")
	}
}

// TestRecordBridgeRoundTripsThroughAGeneratedCodec drives both bridge
// directions through a real mgen-generated record codec (UniFsDirent has a
// bool field — the case that crosses the byte pipe as i32).
func TestRecordBridgeRoundTripsThroughAGeneratedCodec(t *testing.T) {
	original := UniFsDirent{Name: "dir", IsDir: true, Length: 42}

	// encode side: typed record -> ToValue map -> record-case envelope
	wire, err := UniFsDirentToValue(original)
	if err != nil {
		t.Fatalf("UniFsDirentToValue: %v", err)
	}
	envelope, err := RecordFromFieldValues(wire)
	if err != nil {
		t.Fatalf("RecordFromFieldValues: %v", err)
	}

	// the bool crossed as the host-vocabulary i32, u64 as i64
	record := envelope.(UniDataValueRecord)
	if !reflect.DeepEqual(record.Inner[1].FieldValue, scalarI32(1)) {
		t.Fatalf("bool field = %#v, expected i32(1)", record.Inner[1].FieldValue)
	}
	if !reflect.DeepEqual(record.Inner[2].FieldValue, scalarI64(42)) {
		t.Fatalf("u64 field = %#v, expected i64(42)", record.Inner[2].FieldValue)
	}

	// decode side: record-case envelope -> field map -> generated decoder
	fields, err := RecordFieldValues(envelope)
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	decoded, err := UniFsDirentFromValue(fields)
	if err != nil {
		t.Fatalf("UniFsDirentFromValue: %v", err)
	}
	if decoded != original {
		t.Fatalf("decoded = %#v, expected %#v", decoded, original)
	}
}

// TestRecordBridgeFeedsAHostEnvelopeToAGeneratedCodec mirrors the parameter
// direction: the host's uni_from emits an i32 for a bool field, and the
// generated decoder must still read a Go bool out of the bridged map.
func TestRecordBridgeFeedsAHostEnvelopeToAGeneratedCodec(t *testing.T) {
	hostEnvelope := UniDataValueRecord{Inner: []UniDataValueField{
		{FieldName: "", FieldValue: scalarString("dir")},
		{FieldName: "", FieldValue: scalarI32(1)},  // IsDir as the host sends it
		{FieldName: "", FieldValue: scalarI64(42)}, // Length (u64 crosses as i64)
	}}
	fields, err := RecordFieldValues(hostEnvelope)
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	decoded, err := UniFsDirentFromValue(fields)
	if err != nil {
		t.Fatalf("UniFsDirentFromValue: %v", err)
	}
	expected := UniFsDirent{Name: "dir", IsDir: true, Length: 42}
	if decoded != expected {
		t.Fatalf("decoded = %#v, expected %#v", decoded, expected)
	}
}

func TestBoolCheckedAcceptsTheBytePipeIntegerForm(t *testing.T) {
	cases := []struct {
		input    any
		expected bool
	}{
		{true, true},
		{false, false},
		{int64(1), true},
		{int64(0), false},
		{uint64(1), true},
		{int32(0), false},
	}
	for _, c := range cases {
		got, err := boolChecked(c.input)
		if err != nil {
			t.Fatalf("boolChecked(%v): %v", c.input, err)
		}
		if got != c.expected {
			t.Fatalf("boolChecked(%v) = %v, expected %v", c.input, got, c.expected)
		}
	}
	if _, err := boolChecked(int64(2)); err == nil {
		t.Fatal("expected a range error for 2")
	}
	if _, err := boolChecked("yes"); err == nil {
		t.Fatal("expected a type error for a string")
	}
}

func TestExpectMapAcceptsBothMapForms(t *testing.T) {
	untyped := map[any]any{uint64(1): "a"}
	if m, err := expectMap(untyped); err != nil || len(m) != 1 {
		t.Fatalf("expectMap(map[any]any) = %v, %v", m, err)
	}
	typed := map[uint64]any{1: "a", 2: uint64(7)}
	m, err := expectMap(typed)
	if err != nil {
		t.Fatalf("expectMap(map[uint64]any): %v", err)
	}
	if len(m) != 2 || m[uint64(1)] != "a" {
		t.Fatalf("expectMap(map[uint64]any) = %#v", m)
	}
	if _, err := expectMap("nope"); err == nil {
		t.Fatal("expected a type error for a string")
	}
}

// TestRecordFieldValuesPreservesF32 pins the F32 marker through the bridge
// so an f32 field does not come back widened to f64.
func TestRecordFieldValuesPreservesF32(t *testing.T) {
	envelope := UniDataValueRecord{Inner: []UniDataValueField{
		{FieldValue: UniDataValueScalar{Inner: UniScalarValueF32{Inner: 1.5}}},
	}}
	fields, err := RecordFieldValues(envelope)
	if err != nil {
		t.Fatalf("RecordFieldValues: %v", err)
	}
	marked, ok := fields[1].(codec.F32)
	if !ok {
		t.Fatalf("field 1 = %T, expected codec.F32", fields[1])
	}
	back, err := datumFromWireValue(marked)
	if err != nil {
		t.Fatalf("datumFromWireValue: %v", err)
	}
	if !reflect.DeepEqual(back, UniDataValueScalar{Inner: UniScalarValueF32{Inner: 1.5}}) {
		t.Fatalf("roundtrip = %#v", back)
	}
}
