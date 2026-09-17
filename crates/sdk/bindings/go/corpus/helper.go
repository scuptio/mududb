// Package corpus drives the cross-language golden corpus
// (crates/db-kernel/testing/fixtures/golden/v1) through the mgen-generated
// Go codecs and the hand-written codec runtime. The sidecar .json files are
// the single source of truth for the semantic expectations; the .bin
// containers are u32 big-endian length-prefixed segments.
//
// The helpers mirror crates/sdk/bindings/python/corpus_common.py: fixture
// location by walking up from this file, segment unpacking, sidecar
// `expect` shape construction through the generated codec, and the
// deterministic encode inputs pinned byte-exactly against the fixture
// frames.
package corpus

import (
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"testing"

	"github.com/ybbh/mududb_p/bindings/go/types"
)

// ---- fixture location ----

// findFixtureDir locates the repo's golden fixture dir by walking up from
// this source file (no absolute paths hardcoded).
func findFixtureDir(tb testing.TB) string {
	tb.Helper()
	rel := filepath.Join("crates", "db-kernel", "testing", "fixtures", "golden", "v1")
	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		tb.Fatal("runtime.Caller failed")
	}
	d := filepath.Dir(thisFile)
	for {
		candidate := filepath.Join(d, rel)
		if info, err := os.Stat(candidate); err == nil && info.IsDir() {
			return candidate
		}
		parent := filepath.Dir(d)
		if parent == d {
			tb.Fatalf("could not locate %s by walking up from %s", rel, thisFile)
		}
		d = parent
	}
}

// loadFixture returns (segments, sidecar) for a fixture pair
// <stem>.bin / <stem>.json.
func loadFixture(tb testing.TB, fixtureDir, stem string) ([][]byte, map[string]any) {
	tb.Helper()
	corpus, err := os.ReadFile(filepath.Join(fixtureDir, stem+".bin"))
	if err != nil {
		tb.Fatalf("read %s.bin: %v", stem, err)
	}
	sidecarBytes, err := os.ReadFile(filepath.Join(fixtureDir, stem+".json"))
	if err != nil {
		tb.Fatalf("read %s.json: %v", stem, err)
	}
	var sidecar map[string]any
	if err := json.Unmarshal(sidecarBytes, &sidecar); err != nil {
		tb.Fatalf("parse %s.json: %v", stem, err)
	}
	return unpackSegments(tb, corpus), sidecar
}

// unpackSegments splits the length-prefixed container (big-endian u32
// length per segment).
func unpackSegments(tb testing.TB, data []byte) [][]byte {
	tb.Helper()
	var segments [][]byte
	offset := 0
	for offset < len(data) {
		if offset+4 > len(data) {
			tb.Fatalf("truncated segment header at offset %d", offset)
		}
		length := int(binary.BigEndian.Uint32(data[offset : offset+4]))
		offset += 4
		if offset+length > len(data) {
			tb.Fatalf("truncated segment at offset %d (len %d)", offset, length)
		}
		segments = append(segments, data[offset:offset+length])
		offset += length
	}
	return segments
}

// ---- expect comparison (the sidecar is the single source of truth; trees
// are compared through their canonical JSON rendering: map keys sorted,
// u64/i64 as decimal strings, bytes as lowercase hex, numbers plain) ----

// checkExpect compares the rendered expect shape with the sidecar `expect`
// object by canonical JSON equality.
func checkExpect(tb testing.TB, label, what string, actual, expect any) {
	tb.Helper()
	actualJSON, err := json.Marshal(actual)
	if err != nil {
		tb.Fatalf("%s: %s render: %v", label, what, err)
	}
	expectJSON, err := json.Marshal(expect)
	if err != nil {
		tb.Fatalf("%s: %s sidecar: %v", label, what, err)
	}
	if string(actualJSON) != string(expectJSON) {
		tb.Errorf("%s: %s mismatch vs sidecar expect\nactual:  %s\nexpected: %s", label, what, actualJSON, expectJSON)
	}
}

// checkBytes compares two byte slices byte-exactly.
func checkBytes(tb testing.TB, label, what string, actual, expected []byte) {
	tb.Helper()
	if len(actual) != len(expected) {
		tb.Errorf("%s: %s not byte-exact: len %d vs %d", label, what, len(actual), len(expected))
		return
	}
	for i := range actual {
		if actual[i] != expected[i] {
			tb.Errorf("%s: %s not byte-exact: len %d, first diff at %d (0x%02x vs 0x%02x)",
				label, what, len(actual), i, actual[i], expected[i])
			return
		}
	}
}

// ---- sidecar `expect` shape construction (conventions: u64/i64/u128 as
// decimal strings, u8/u32 as plain JSON numbers, bytes as lowercase hex,
// UniOid as {"h","l"} decimal strings, unit as {"unit": true}, relation
// cells as hex-or-null) ----

func u64Str(v uint64) string { return strconv.FormatUint(v, 10) }
func i64Str(v int64) string  { return strconv.FormatInt(v, 10) }
func hexStr(b []byte) string { return hex.EncodeToString(b) }

func oidJSON(o types.UniOid) map[string]any {
	return map[string]any{"h": u64Str(o.H), "l": u64Str(o.L)}
}

func attrDatumJSON(attr uint64, datum []byte) map[string]any {
	return map[string]any{"attr": u64Str(attr), "datum_hex": hexStr(datum)}
}

// attrDatumListJSON renders a relation key/value list (tuple surface:
// []any{uint64, []byte}).
func attrDatumListJSON(list [][]any) ([]any, error) {
	out := make([]any, 0, len(list))
	for _, item := range list {
		attr, ok := item[0].(uint64)
		if !ok {
			return nil, fmt.Errorf("attr datum attr is %T, expected uint64", item[0])
		}
		datum, ok := item[1].([]byte)
		if !ok {
			return nil, fmt.Errorf("attr datum datum is %T, expected []byte", item[1])
		}
		out = append(out, attrDatumJSON(attr, datum))
	}
	return out, nil
}

var scalarNames = map[types.UniScalar]string{
	types.UniScalarBool:        "bool",
	types.UniScalarU8:          "u8",
	types.UniScalarI8:          "i8",
	types.UniScalarU16:         "u16",
	types.UniScalarI16:         "i16",
	types.UniScalarU32:         "u32",
	types.UniScalarI32:         "i32",
	types.UniScalarU64:         "u64",
	types.UniScalarU128:        "u128",
	types.UniScalarI64:         "i64",
	types.UniScalarI128:        "i128",
	types.UniScalarF32:         "f32",
	types.UniScalarF64:         "f64",
	types.UniScalarChar:        "char",
	types.UniScalarString:      "string",
	types.UniScalarBlob:        "blob",
	types.UniScalarNumeric:     "numeric",
	types.UniScalarDate:        "date",
	types.UniScalarTime:        "time",
	types.UniScalarTimestamp:   "timestamp",
	types.UniScalarTimestampTz: "timestamp_tz",
}

func dataTypeName(t types.UniDataType) (string, error) {
	// A UniDataType left at its proto3-style default (an absent variant
	// field) decodes as the first case, scalar(bool), matching the host.
	s, ok := t.(types.UniDataTypeScalar)
	if !ok {
		return "", fmt.Errorf("field_type serialization not covered by the sidecar conventions: %T", t)
	}
	name, ok := scalarNames[s.Inner]
	if !ok {
		return "", fmt.Errorf("unknown scalar %d", s.Inner)
	}
	return "scalar(" + name + ")", nil
}

func recordFieldJSON(f types.UniRecordField) (map[string]any, error) {
	typeName, err := dataTypeName(f.FieldType)
	if err != nil {
		return nil, err
	}
	attrs := make([]any, 0, len(f.FieldAttrs))
	for _, a := range f.FieldAttrs {
		attrs = append(attrs, map[string]any{"attr_name": a.AttrName, "attr_value": a.AttrValue})
	}
	return map[string]any{
		"field_name":  f.FieldName,
		"field_type":  typeName,
		"field_attrs": attrs,
	}, nil
}

func sqlParamsJSON(params types.UniSqlParam) ([]any, error) {
	// The sidecar conventions do not define a rendering for UniDataValue
	// params; every corpus/lenient frame carries an empty parameter list.
	if len(params.Params) != 0 {
		return nil, errors.New("sql params serialization not covered by the sidecar conventions")
	}
	return []any{}, nil
}

func sqlArgvJSON(oid types.UniOid, sql string, params types.UniSqlParam) (map[string]any, error) {
	p, err := sqlParamsJSON(params)
	if err != nil {
		return nil, err
	}
	return map[string]any{"oid": oidJSON(oid), "sql": sql, "params": p}, nil
}

func queryResultJSON(r types.UniQueryResult) (map[string]any, error) {
	// The sidecar conventions do not define a rendering for query rows;
	// every corpus/lenient frame carries an empty row set.
	if len(r.ResultSet.RowSet) != 0 {
		return nil, errors.New("query row serialization not covered by the sidecar conventions")
	}
	fields := make([]any, 0, len(r.TupleDesc.RecordFields))
	for _, f := range r.TupleDesc.RecordFields {
		fj, err := recordFieldJSON(f)
		if err != nil {
			return nil, err
		}
		fields = append(fields, fj)
	}
	return map[string]any{
		"record_name": r.TupleDesc.RecordName,
		"fields":      fields,
		"eof":         r.ResultSet.Eof,
		"rows":        []any{},
		"cursor_hex":  hexStr(r.ResultSet.Cursor),
	}, nil
}

func fsStatJSON(s types.UniFsStat) map[string]any {
	return map[string]any{
		"oid":        oidJSON(s.Oid),
		"generation": u64Str(s.Generation),
		"entry":      s.Entry,
		"length":     u64Str(s.Length),
		"state":      uint64(s.State),
	}
}

func direntsJSON(entries []types.UniFsDirent) []any {
	out := make([]any, 0, len(entries))
	for _, d := range entries {
		out = append(out, map[string]any{
			"name": d.Name, "is_dir": d.IsDir, "length": u64Str(d.Length),
		})
	}
	return out
}

// relationRowJSON renders a decoded relation row ([]*[]byte, nil cell =
// JSON null) or JSON null for an absent row.
func relationRowJSON(row []*[]byte) any {
	if row == nil {
		return nil
	}
	out := make([]any, 0, len(row))
	for _, cell := range row {
		if cell == nil {
			out = append(out, nil)
		} else {
			out = append(out, hexStr(*cell))
		}
	}
	return out
}

func errJSON(err types.UniError) map[string]any {
	return map[string]any{
		"err_code":        uint64(err.ErrCode),
		"err_msg":         err.ErrMsg,
		"err_src":         err.ErrSrc,
		"err_loc":         err.ErrLoc,
		"err_details_hex": hexStr(err.ErrDetails),
	}
}

var unitJSON = map[string]any{"unit": true}

// ---- decode a frame and render the decoded values in the sidecar expect
// shape ----

func expectOk(r types.WireResult, what string) (any, error) {
	if !r.IsOk() {
		return nil, fmt.Errorf("expected ok %s result, got err %v", what, errJSON(*r.Error))
	}
	return r.Value, nil
}

func headerKind(frame []byte) (uint32, error) {
	kind, err := types.DecodeHeader(frame)
	if err != nil {
		return 0, err
	}
	return uint32(kind), nil
}

func decodeRequestExpect(kind uint32, frame []byte) (any, error) {
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		v, err := types.DecodeQueryRequest(frame)
		if err != nil {
			return nil, err
		}
		return sqlArgvJSON(v.Oid, v.Query.SqlString, v.ParamList)
	case types.MessageKindCommand:
		v, err := types.DecodeCommandRequest(frame)
		if err != nil {
			return nil, err
		}
		return sqlArgvJSON(v.Oid, v.Command.SqlString, v.ParamList)
	case types.MessageKindBatch:
		v, err := types.DecodeBatchRequest(frame)
		if err != nil {
			return nil, err
		}
		return sqlArgvJSON(v.Oid, v.Command.SqlString, v.ParamList)
	case types.MessageKindOpenSession:
		v, err := types.DecodeOpenSessionRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"worker_oid": oidJSON(v)}, nil
	case types.MessageKindCloseSession:
		v, err := types.DecodeCloseSessionRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(v)}, nil
	case types.MessageKindGet:
		oid, key, err := types.DecodeGetRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "key_hex": hexStr(key)}, nil
	case types.MessageKindPut:
		oid, key, value, err := types.DecodePutRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "key_hex": hexStr(key), "value_hex": hexStr(value)}, nil
	case types.MessageKindDelete:
		oid, key, err := types.DecodeDeleteRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "key_hex": hexStr(key)}, nil
	case types.MessageKindRange:
		oid, start, end, err := types.DecodeRangeRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "start_hex": hexStr(start), "end_hex": hexStr(end)}, nil
	case types.MessageKindFsOpen:
		v, err := types.DecodeFsOpenRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{
			"session": oidJSON(v.Session),
			"oid":     oidJSON(v.Oid),
			"path":    v.Path,
			"flags":   uint64(v.Flags),
		}, nil
	case types.MessageKindFsClose:
		fd, err := types.DecodeFsCloseRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd)}, nil
	case types.MessageKindFsRead:
		fd, length, err := types.DecodeFsReadRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd), "len": uint64(length)}, nil
	case types.MessageKindFsWrite:
		fd, data, err := types.DecodeFsWriteRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd), "data_hex": hexStr(data)}, nil
	case types.MessageKindFsPread:
		fd, offset, length, err := types.DecodeFsPreadRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd), "offset": u64Str(offset), "len": uint64(length)}, nil
	case types.MessageKindFsPwrite:
		fd, offset, data, err := types.DecodeFsPwriteRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd), "offset": u64Str(offset), "data_hex": hexStr(data)}, nil
	case types.MessageKindFsLseek:
		fd, offset, whence, err := types.DecodeFsLseekRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd), "offset": i64Str(offset), "whence": uint64(whence)}, nil
	case types.MessageKindFsFstat:
		fd, err := types.DecodeFsFstatRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd)}, nil
	case types.MessageKindFsStat:
		oid, path, err := types.DecodeFsStatRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "path": path}, nil
	case types.MessageKindFsFsync:
		fd, err := types.DecodeFsFsyncRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"fd": uint64(fd)}, nil
	case types.MessageKindFsReaddir:
		oid, path, err := types.DecodeFsReaddirRequest(frame)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "path": path}, nil
	case types.MessageKindRelationGet:
		oid, table, key, select_, err := types.DecodeRelationGetRequest(frame)
		if err != nil {
			return nil, err
		}
		keyJSON, err := attrDatumListJSON(key)
		if err != nil {
			return nil, err
		}
		sel := make([]any, 0, len(select_))
		for _, s := range select_ {
			sel = append(sel, u64Str(s))
		}
		return map[string]any{"oid": oidJSON(oid), "table": table, "key": keyJSON, "select": sel}, nil
	case types.MessageKindRelationUpdate:
		oid, table, key, values, deltas, err := types.DecodeRelationUpdateRequest(frame)
		if err != nil {
			return nil, err
		}
		keyJSON, err := attrDatumListJSON(key)
		if err != nil {
			return nil, err
		}
		valuesJSON, err := attrDatumListJSON(values)
		if err != nil {
			return nil, err
		}
		deltasJSON := make([]any, 0, len(deltas))
		for _, item := range deltas {
			attr, ok := item[0].(uint64)
			if !ok {
				return nil, fmt.Errorf("delta attr is %T, expected uint64", item[0])
			}
			op, ok := item[1].(uint8)
			if !ok {
				return nil, fmt.Errorf("delta op is %T, expected uint8", item[1])
			}
			datum, ok := item[2].([]byte)
			if !ok {
				return nil, fmt.Errorf("delta datum is %T, expected []byte", item[2])
			}
			deltasJSON = append(deltasJSON, map[string]any{
				"attr": u64Str(attr), "op": uint64(op), "datum_hex": hexStr(datum),
			})
		}
		return map[string]any{
			"oid": oidJSON(oid), "table": table, "key": keyJSON, "values": valuesJSON, "deltas": deltasJSON,
		}, nil
	case types.MessageKindRelationInsert:
		oid, table, key, values, err := types.DecodeRelationInsertRequest(frame)
		if err != nil {
			return nil, err
		}
		keyJSON, err := attrDatumListJSON(key)
		if err != nil {
			return nil, err
		}
		valuesJSON, err := attrDatumListJSON(values)
		if err != nil {
			return nil, err
		}
		return map[string]any{"oid": oidJSON(oid), "table": table, "key": keyJSON, "values": valuesJSON}, nil
	default:
		return nil, fmt.Errorf("unknown request kind %d", kind)
	}
}

func decodeResponseExpect(kind uint32, frame []byte) (any, error) {
	r, err := decodeResponse(kind, frame)
	if err != nil {
		return nil, err
	}
	v, err := expectOk(r, fmt.Sprintf("kind-%d", kind))
	if err != nil {
		return nil, err
	}
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		return queryResultJSON(v.(types.UniQueryResult))
	case types.MessageKindCommand:
		return map[string]any{"affected_rows": u64Str(v.(types.UniCommandResult).AffectedRows)}, nil
	case types.MessageKindBatch:
		return map[string]any{"affected_rows": u64Str(v.(types.UniCommandResult).AffectedRows)}, nil
	case types.MessageKindOpenSession:
		oid := v.(types.UniOid)
		// The sidecar renders the session UniOid as its u128 decimal value,
		// which equals l for every h == 0 session id used by the fixtures.
		if oid.H != 0 {
			return nil, errors.New("session oid serialization not covered by the sidecar conventions")
		}
		return map[string]any{"session": u64Str(oid.L)}, nil
	case types.MessageKindCloseSession:
		return unitJSON, nil
	case types.MessageKindGet:
		if v == nil {
			return nil, errors.New("absent get value serialization not covered by the sidecar conventions")
		}
		return map[string]any{"value_hex": hexStr(v.([]byte))}, nil
	case types.MessageKindPut:
		return unitJSON, nil
	case types.MessageKindDelete:
		return unitJSON, nil
	case types.MessageKindRange:
		items := make([]any, 0)
		for _, item := range v.([][]any) {
			items = append(items, map[string]any{
				"key_hex":   hexStr(item[0].([]byte)),
				"value_hex": hexStr(item[1].([]byte)),
			})
		}
		return map[string]any{"items": items}, nil
	case types.MessageKindFsOpen:
		return map[string]any{"fd": uint64(v.(uint32))}, nil
	case types.MessageKindFsClose:
		return unitJSON, nil
	case types.MessageKindFsRead:
		return map[string]any{"data_hex": hexStr(v.([]byte))}, nil
	case types.MessageKindFsWrite:
		return map[string]any{"written": uint64(v.(uint32))}, nil
	case types.MessageKindFsPread:
		return map[string]any{"data_hex": hexStr(v.([]byte))}, nil
	case types.MessageKindFsPwrite:
		return unitJSON, nil
	case types.MessageKindFsLseek:
		return map[string]any{"position": u64Str(v.(uint64))}, nil
	case types.MessageKindFsFstat:
		return map[string]any{"stat": fsStatJSON(v.(types.UniFsStat))}, nil
	case types.MessageKindFsStat:
		return map[string]any{"stat": fsStatJSON(v.(types.UniFsStat))}, nil
	case types.MessageKindFsFsync:
		return unitJSON, nil
	case types.MessageKindFsReaddir:
		return map[string]any{"entries": direntsJSON(v.([]types.UniFsDirent))}, nil
	case types.MessageKindRelationGet:
		if v == nil {
			return map[string]any{"row": nil}, nil
		}
		return map[string]any{"row": relationRowJSON(v.([]*[]byte))}, nil
	case types.MessageKindRelationUpdate:
		return map[string]any{"affected": u64Str(v.(uint64))}, nil
	case types.MessageKindRelationInsert:
		return unitJSON, nil
	default:
		return nil, fmt.Errorf("unknown response kind %d", kind)
	}
}

func decodeErrExpect(kind uint32, frame []byte) (any, error) {
	r, err := decodeResponse(kind, frame)
	if err != nil {
		return nil, err
	}
	if r.IsOk() {
		return nil, fmt.Errorf("expected err %d result, got ok", kind)
	}
	return errJSON(*r.Error), nil
}

// ---- deterministic encode inputs pinned byte-exactly against the fixture
// frames (mirrors the Python corpus_common encode inputs) ----

func oid(l uint64) types.UniOid { return types.UniOid{H: 0, L: l} }

func queryArgv(l uint64, sql string) types.UniQueryArgv {
	return types.UniQueryArgv{
		Oid:       oid(l),
		Query:     types.UniSqlStmt{SqlString: sql},
		ParamList: types.UniSqlParam{},
	}
}

func commandArgv(l uint64, sql string) types.UniCommandArgv {
	return types.UniCommandArgv{
		Oid:       oid(l),
		Command:   types.UniSqlStmt{SqlString: sql},
		ParamList: types.UniSqlParam{},
	}
}

func fsOpenArgv() types.UniFsOpenArgv {
	return types.UniFsOpenArgv{Session: oid(10), Oid: oid(11), Path: "docs/a.txt", Flags: 2}
}

func goldenFsStat() types.UniFsStat {
	return types.UniFsStat{Oid: oid(5), Generation: 1, Entry: "", Length: 100, State: 1}
}

func goldenDirents() []types.UniFsDirent {
	return []types.UniFsDirent{
		{Name: "a.txt", IsDir: false, Length: 3},
		{Name: "docs", IsDir: true, Length: 0},
	}
}

// goldenErr is the trailing corpus frame: get result Err(NotFound,
// "no such entry"). ErrSrc is the host's JSON form of ErrorSource::None;
// ErrDetails encodes as a MessagePack ARRAY, not bin — both quirks pinned
// on purpose.
func goldenErr() types.UniError {
	return types.UniError{ErrCode: 2, ErrMsg: "no such entry", ErrSrc: "\"None\"", ErrLoc: "", ErrDetails: []byte{}}
}

func attrDatum(attr uint64, datum ...byte) []any {
	return []any{attr, datum}
}

func encodeRequest(kind uint32) ([]byte, error) {
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		return types.EncodeQueryRequest(queryArgv(1, "select 1"))
	case types.MessageKindCommand:
		return types.EncodeCommandRequest(commandArgv(2, "update t set a = 1"))
	case types.MessageKindBatch:
		return types.EncodeBatchRequest(commandArgv(3, "insert into t values (1)"))
	case types.MessageKindOpenSession:
		return types.EncodeOpenSessionRequest(oid(4))
	case types.MessageKindCloseSession:
		return types.EncodeCloseSessionRequest(oid(5))
	case types.MessageKindGet:
		return types.EncodeGetRequest(oid(6), []byte("k1"))
	case types.MessageKindPut:
		return types.EncodePutRequest(oid(7), []byte("k1"), []byte("v1"))
	case types.MessageKindDelete:
		return types.EncodeDeleteRequest(oid(8), []byte("k1"))
	case types.MessageKindRange:
		return types.EncodeRangeRequest(oid(9), []byte("a"), []byte("z"))
	case types.MessageKindFsOpen:
		return types.EncodeFsOpenRequest(fsOpenArgv())
	case types.MessageKindFsClose:
		return types.EncodeFsCloseRequest(3)
	case types.MessageKindFsRead:
		return types.EncodeFsReadRequest(3, 4)
	case types.MessageKindFsWrite:
		return types.EncodeFsWriteRequest(3, []byte("hi"))
	case types.MessageKindFsPread:
		return types.EncodeFsPreadRequest(3, 8, 4)
	case types.MessageKindFsPwrite:
		return types.EncodeFsPwriteRequest(3, 8, []byte("hi"))
	case types.MessageKindFsLseek:
		return types.EncodeFsLseekRequest(3, -2, 1)
	case types.MessageKindFsFstat:
		return types.EncodeFsFstatRequest(3)
	case types.MessageKindFsStat:
		return types.EncodeFsStatRequest(oid(18), "a")
	case types.MessageKindFsFsync:
		return types.EncodeFsFsyncRequest(3)
	case types.MessageKindFsReaddir:
		return types.EncodeFsReaddirRequest(oid(20), "d")
	case types.MessageKindRelationGet:
		return types.EncodeRelationGetRequest(
			oid(21), "t",
			[][]any{attrDatum(1, 0x01), attrDatum(2, 0x02, 0x03)},
			[]uint64{3, 4},
		)
	case types.MessageKindRelationUpdate:
		return types.EncodeRelationUpdateRequest(
			oid(22), "t",
			[][]any{attrDatum(1, 0x01)},
			[][]any{attrDatum(2, 0x0a)},
			[][]any{{uint64(3), uint8(0), []byte{0x05}}},
		)
	case types.MessageKindRelationInsert:
		return types.EncodeRelationInsertRequest(
			oid(23), "t",
			[][]any{attrDatum(1, 0x01)},
			[][]any{attrDatum(2, 0x0a)},
		)
	default:
		return nil, fmt.Errorf("unknown request kind %d", kind)
	}
}

// okResponseValue returns the deterministic ok payload per message kind.
func okResponseValue(kind uint32) (any, error) {
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		return types.UniQueryResult{}, nil
	case types.MessageKindCommand:
		return types.UniCommandResult{AffectedRows: 3}, nil
	case types.MessageKindBatch:
		return types.UniCommandResult{AffectedRows: 2}, nil
	case types.MessageKindOpenSession:
		return oid(4), nil
	case types.MessageKindGet:
		return []byte("v1"), nil
	case types.MessageKindRange:
		return [][]any{
			{[]byte("a"), []byte("1")},
			{[]byte("b"), []byte("2")},
		}, nil
	case types.MessageKindFsOpen:
		return uint32(9), nil
	case types.MessageKindFsRead:
		return []byte("hi"), nil
	case types.MessageKindFsWrite:
		return uint32(2), nil
	case types.MessageKindFsPread:
		return []byte("hi"), nil
	case types.MessageKindFsLseek:
		return uint64(6), nil
	case types.MessageKindFsFstat:
		return goldenFsStat(), nil
	case types.MessageKindFsStat:
		return goldenFsStat(), nil
	case types.MessageKindFsReaddir:
		return goldenDirents(), nil
	case types.MessageKindRelationGet:
		cell := []byte{0x0a}
		return []*[]byte{&cell, nil}, nil
	case types.MessageKindRelationUpdate:
		return uint64(1), nil
	case types.MessageKindCloseSession,
		types.MessageKindPut,
		types.MessageKindDelete,
		types.MessageKindFsClose,
		types.MessageKindFsPwrite,
		types.MessageKindFsFsync,
		types.MessageKindRelationInsert:
		return nil, nil
	default:
		return nil, fmt.Errorf("unknown response kind %d", kind)
	}
}

func encodeOkResponse(kind uint32) ([]byte, error) {
	value, err := okResponseValue(kind)
	if err != nil {
		return nil, err
	}
	return encodeResponse(kind, types.OkResult(value))
}

func encodeGetErrResponse() ([]byte, error) {
	return types.EncodeGetResult(types.ErrResult(goldenErr()))
}

func encodeResponse(kind uint32, result types.WireResult) ([]byte, error) {
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		return types.EncodeQueryResult(result)
	case types.MessageKindCommand:
		return types.EncodeCommandResult(result)
	case types.MessageKindBatch:
		return types.EncodeBatchResult(result)
	case types.MessageKindOpenSession:
		return types.EncodeOpenSessionResult(result)
	case types.MessageKindCloseSession:
		return types.EncodeCloseSessionResult(result)
	case types.MessageKindGet:
		return types.EncodeGetResult(result)
	case types.MessageKindPut:
		return types.EncodePutResult(result)
	case types.MessageKindDelete:
		return types.EncodeDeleteResult(result)
	case types.MessageKindRange:
		return types.EncodeRangeResult(result)
	case types.MessageKindFsOpen:
		return types.EncodeFsOpenResult(result)
	case types.MessageKindFsClose:
		return types.EncodeFsCloseResult(result)
	case types.MessageKindFsRead:
		return types.EncodeFsReadResult(result)
	case types.MessageKindFsWrite:
		return types.EncodeFsWriteResult(result)
	case types.MessageKindFsPread:
		return types.EncodeFsPreadResult(result)
	case types.MessageKindFsPwrite:
		return types.EncodeFsPwriteResult(result)
	case types.MessageKindFsLseek:
		return types.EncodeFsLseekResult(result)
	case types.MessageKindFsFstat:
		return types.EncodeFsFstatResult(result)
	case types.MessageKindFsStat:
		return types.EncodeFsStatResult(result)
	case types.MessageKindFsFsync:
		return types.EncodeFsFsyncResult(result)
	case types.MessageKindFsReaddir:
		return types.EncodeFsReaddirResult(result)
	case types.MessageKindRelationGet:
		return types.EncodeRelationGetResult(result)
	case types.MessageKindRelationUpdate:
		return types.EncodeRelationUpdateResult(result)
	case types.MessageKindRelationInsert:
		return types.EncodeRelationInsertResult(result)
	default:
		return nil, fmt.Errorf("unknown response kind %d", kind)
	}
}

func decodeResponse(kind uint32, frame []byte) (types.WireResult, error) {
	switch types.MessageKind(kind) {
	case types.MessageKindQuery:
		return types.DecodeQueryResult(frame)
	case types.MessageKindCommand:
		return types.DecodeCommandResult(frame)
	case types.MessageKindBatch:
		return types.DecodeBatchResult(frame)
	case types.MessageKindOpenSession:
		return types.DecodeOpenSessionResult(frame)
	case types.MessageKindCloseSession:
		return types.DecodeCloseSessionResult(frame)
	case types.MessageKindGet:
		return types.DecodeGetResult(frame)
	case types.MessageKindPut:
		return types.DecodePutResult(frame)
	case types.MessageKindDelete:
		return types.DecodeDeleteResult(frame)
	case types.MessageKindRange:
		return types.DecodeRangeResult(frame)
	case types.MessageKindFsOpen:
		return types.DecodeFsOpenResult(frame)
	case types.MessageKindFsClose:
		return types.DecodeFsCloseResult(frame)
	case types.MessageKindFsRead:
		return types.DecodeFsReadResult(frame)
	case types.MessageKindFsWrite:
		return types.DecodeFsWriteResult(frame)
	case types.MessageKindFsPread:
		return types.DecodeFsPreadResult(frame)
	case types.MessageKindFsPwrite:
		return types.DecodeFsPwriteResult(frame)
	case types.MessageKindFsLseek:
		return types.DecodeFsLseekResult(frame)
	case types.MessageKindFsFstat:
		return types.DecodeFsFstatResult(frame)
	case types.MessageKindFsStat:
		return types.DecodeFsStatResult(frame)
	case types.MessageKindFsFsync:
		return types.DecodeFsFsyncResult(frame)
	case types.MessageKindFsReaddir:
		return types.DecodeFsReaddirResult(frame)
	case types.MessageKindRelationGet:
		return types.DecodeRelationGetResult(frame)
	case types.MessageKindRelationUpdate:
		return types.DecodeRelationUpdateResult(frame)
	case types.MessageKindRelationInsert:
		return types.DecodeRelationInsertResult(frame)
	default:
		return types.WireResult{}, fmt.Errorf("unknown response kind %d", kind)
	}
}

// jsonString extracts a string field of a sidecar vector/frame.
func jsonString(m map[string]any, key string) string {
	if s, ok := m[key].(string); ok {
		return s
	}
	return ""
}

// jsonUint extracts an integer field of a sidecar vector/frame (JSON
// numbers decode as float64).
func jsonUint(m map[string]any, key string) uint64 {
	if f, ok := m[key].(float64); ok {
		return uint64(f)
	}
	return 0
}
