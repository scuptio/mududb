package main

import (
	"errors"
	"fmt"

	"wallet_go/binding/mududb/api/system"

	"github.com/ybbh/mududb_p/bindings/go/codec"
	"github.com/ybbh/mududb_p/bindings/go/types"
	"go.bytecodealliance.org/cm"
)

// Syscall layer for the wallet-go guest: the `mududb:api/system` byte pipe
// framed with the in-repo Go binding (`types` for the Uni* DTOs and the MSSP
// func codecs, `codec` for the MessagePack/MSSP frame runtime), mirroring
// the Rust SDK's `SyscallPayload`/`MuduSysCallApi` semantics. There is no Go
// db facade: the procedures encode query/command syscalls with `types` +
// `codec` directly.
//
// The procedure byte pipe (the `mp2-*` exports wired in the mtp-generated
// main_gen.go) is NOT MSSP-framed: the host passes a bare MessagePack
// `UniProcedureParam` (record-as-map) and expects the hand-written
// `UniResult<UniProcedureResult, UniError>` shape back — a single-entry map,
// key 0 for the ok value and key 1 for the error (see
// `mudu_binding::universal::uni_result::UniResult` on the host).

// muduOid is the session OID; an alias of the binding's UniOid so the
// decoded procedure parameter feeds straight into the syscall DTOs.
type muduOid = types.UniOid

// uniError is the error record carried by the `[1, UniError]` result arm.
type uniError struct {
	Code    uint32
	Message string
}

// uniHostError is a host-side uni error surfaced to the procedure; the
// byte-pipe dispatcher re-encodes it as the procedure result's error arm.
type uniHostError struct {
	uni uniError
}

func (e *uniHostError) Error() string { return e.uni.Message }

// domainError is a domain error raised by a procedure; the byte-pipe
// dispatcher encodes it as the `{1: UniError}` arm of the procedure result.
type domainError struct {
	code    uint32
	message string
}

func (e *domainError) Error() string { return e.message }

// Numeric mirrors of the host's `mudu::error::ErrorCode` discriminants
// carried in the `UniError` result arm.
const (
	errCodeEntityNotFound  uint32 = 50009
	errCodeDomainViolation uint32 = 50017
	errCodeInvalidArgument uint32 = 50029
	errCodeInternal        uint32 = 50000
)

// sysQuery runs a SQL query and returns the rows, each cell decoded to a
// native Go value (int64 / float64 / bool / string / []byte / nil). A host
// error comes back as `*uniHostError`.
func sysQuery(oid muduOid, sql string, args ...any) ([][]any, error) {
	params, err := nativeParams(args)
	if err != nil {
		return nil, err
	}
	frame, err := types.EncodeQueryRequest(types.UniQueryArgv{
		Oid:       oid,
		Query:     types.UniSqlStmt{SqlString: sql},
		ParamList: types.UniSqlParam{Params: params},
	})
	if err != nil {
		return nil, err
	}
	response := system.Query(cm.ToList(frame))
	result, err := types.DecodeQueryResult(response.Slice())
	if err != nil {
		return nil, err
	}
	if result.Error != nil {
		return nil, hostError(result.Error)
	}
	queryResult, ok := result.Value.(types.UniQueryResult)
	if !ok {
		return nil, fmt.Errorf("MSSP: query result payload is %T, not UniQueryResult", result.Value)
	}

	var rows [][]any
	for _, row := range queryResult.ResultSet.RowSet {
		fields := make([]any, 0, len(row.Fields))
		for _, datum := range row.Fields {
			value, err := datumToNative(datum)
			if err != nil {
				return nil, err
			}
			fields = append(fields, value)
		}
		rows = append(rows, fields)
	}
	return rows, nil
}

// sysCommand runs a SQL command and returns affected rows. A host error
// comes back as `*uniHostError`.
func sysCommand(oid muduOid, sql string, args ...any) (uint64, error) {
	params, err := nativeParams(args)
	if err != nil {
		return 0, err
	}
	frame, err := types.EncodeCommandRequest(types.UniCommandArgv{
		Oid:       oid,
		Command:   types.UniSqlStmt{SqlString: sql},
		ParamList: types.UniSqlParam{Params: params},
	})
	if err != nil {
		return 0, err
	}
	response := system.Command(cm.ToList(frame))
	result, err := types.DecodeCommandResult(response.Slice())
	if err != nil {
		return 0, err
	}
	if result.Error != nil {
		return 0, hostError(result.Error)
	}
	commandResult, ok := result.Value.(types.UniCommandResult)
	if !ok {
		return 0, fmt.Errorf("MSSP: command result payload is %T, not UniCommandResult", result.Value)
	}
	return commandResult.AffectedRows, nil
}

// ---- procedure byte-pipe codec (`handle_procedure`) ----

// procedureParam is the decoded `UniProcedureParam` the host passes to the
// `mp2-*` exports; scalar parameters are unwrapped to native Go values (the
// shapes the mtp Go front-end supports: bool / int64 / float64 / string /
// []byte / nil).
type procedureParam struct {
	Procedure uint64
	Session   muduOid
	Params    []any
}

func decodeProcedureParam(b []byte) (procedureParam, error) {
	reader := codec.NewMpackReader(b)
	value, err := reader.ReadValue()
	if err != nil {
		return procedureParam{}, err
	}
	if !reader.IsDone() {
		return procedureParam{}, errors.New("trailing bytes after UniProcedureParam")
	}
	decoded, err := types.UniProcedureParamFromValue(value)
	if err != nil {
		return procedureParam{}, err
	}
	params := make([]any, 0, len(decoded.ParamList))
	for _, datum := range decoded.ParamList {
		native, err := datumToNative(datum)
		if err != nil {
			return procedureParam{}, err
		}
		params = append(params, native)
	}
	return procedureParam{
		Procedure: decoded.Procedure,
		Session:   decoded.Session,
		Params:    params,
	}, nil
}

// encodeProcedureOk encodes the ok arm of
// `UniResult<UniProcedureResult, UniError>`: the single-entry map
// `{0: {1: [return_list]}}`.
func encodeProcedureOk(value any) []byte {
	datum, err := nativeToDatum(value)
	if err != nil {
		return encodeProcedureError(errCodeInternal, err.Error())
	}
	wire, err := types.UniProcedureResultToValue(types.UniProcedureResult{
		ReturnList: []types.UniDataValue{datum},
	})
	if err != nil {
		return encodeProcedureError(errCodeInternal, err.Error())
	}
	writer := &codec.MpackWriter{}
	if err := writer.WriteValue(map[uint64]any{0: wire}); err != nil {
		return encodeProcedureError(errCodeInternal, err.Error())
	}
	return writer.Bytes()
}

// encodeProcedureError encodes the err arm: `{1: UniError}`.
func encodeProcedureError(code uint32, message string) []byte {
	wire, err := types.UniErrorToValue(types.UniError{
		ErrCode: code,
		ErrMsg:  message,
		ErrSrc:  "wallet-go",
	})
	if err != nil {
		// UniErrorToValue cannot fail on plain string fields; guard anyway.
		writer := &codec.MpackWriter{}
		_ = writer.WriteValue(map[uint64]any{1: map[uint64]any{2: "encode error"}})
		return writer.Bytes()
	}
	writer := &codec.MpackWriter{}
	if err := writer.WriteValue(map[uint64]any{1: wire}); err != nil {
		panic(err)
	}
	return writer.Bytes()
}

// ---- adapter cast helpers (used by the mtp-generated wiring) ----

func asI64(v any) (int64, error) {
	i, ok := v.(int64)
	if !ok {
		return 0, &domainError{errCodeInvalidArgument, "expected an i64 parameter"}
	}
	return i, nil
}

func asString(v any) (string, error) {
	s, ok := v.(string)
	if !ok {
		return "", &domainError{errCodeInvalidArgument, "expected a string parameter"}
	}
	return s, nil
}

// ---- internals ----

func hostError(uniErr *types.UniError) *uniHostError {
	return &uniHostError{uniError{Code: uniErr.ErrCode, Message: uniErr.ErrMsg}}
}

// nativeToDatum wraps a native Go value as a UniDataValue for the syscall
// parameter list and the procedure result. A value already in UniDataValue
// form (the record-case envelope the mtp adapter builds for a record-typed
// result with types.RecordFromFieldValues) passes through unchanged.
func nativeToDatum(v any) (types.UniDataValue, error) {
	switch value := v.(type) {
	case types.UniDataValue:
		return value, nil
	case nil:
		return types.UniDataValueScalar{Inner: types.UniScalarValueNull{}}, nil
	case int64:
		return types.UniDataValueScalar{Inner: types.UniScalarValueI64{Inner: value}}, nil
	case int:
		return types.UniDataValueScalar{Inner: types.UniScalarValueI64{Inner: int64(value)}}, nil
	case float64:
		return types.UniDataValueScalar{Inner: types.UniScalarValueF64{Inner: value}}, nil
	case bool:
		return types.UniDataValueScalar{Inner: types.UniScalarValueBool{Inner: value}}, nil
	case string:
		return types.UniDataValueScalar{Inner: types.UniScalarValueString{Inner: value}}, nil
	case []byte:
		return types.UniDataValueScalar{Inner: types.UniScalarValueBlob{Inner: value}}, nil
	default:
		return nil, fmt.Errorf("unsupported value type for the wire: %T", v)
	}
}

// datumToNative unwraps a scalar UniDataValue to its native Go value; every
// integer width collapses to int64 (the procedure type table has no narrower
// ints). Composite datums (record/array/binary cases — user-defined
// procedure types) keep their UniDataValue form: the mtp adapter unwraps
// them through the binding record bridge (types.RecordFieldValues).
func datumToNative(datum types.UniDataValue) (any, error) {
	scalar, ok := datum.(types.UniDataValueScalar)
	if !ok {
		switch datum.(type) {
		case types.UniDataValueRecord, types.UniDataValueArray, types.UniDataValueBinary:
			return datum, nil
		default:
			return nil, fmt.Errorf("unsupported UniDataValue kind %T", datum)
		}
	}
	switch value := scalar.Inner.(type) {
	case types.UniScalarValueBool:
		return value.Inner, nil
	case types.UniScalarValueU8:
		return int64(value.Inner), nil
	case types.UniScalarValueI8:
		return int64(value.Inner), nil
	case types.UniScalarValueU16:
		return int64(value.Inner), nil
	case types.UniScalarValueI16:
		return int64(value.Inner), nil
	case types.UniScalarValueU32:
		return int64(value.Inner), nil
	case types.UniScalarValueI32:
		return int64(value.Inner), nil
	case types.UniScalarValueU64:
		return int64(value.Inner), nil
	case types.UniScalarValueI64:
		return value.Inner, nil
	case types.UniScalarValueF32:
		return float64(value.Inner), nil
	case types.UniScalarValueF64:
		return value.Inner, nil
	case types.UniScalarValueChar:
		return value.Inner, nil
	case types.UniScalarValueString:
		return value.Inner, nil
	case types.UniScalarValueNumeric:
		return value.Inner, nil
	case types.UniScalarValueBlob:
		return value.Inner, nil
	case types.UniScalarValueNull:
		return nil, nil
	default:
		return nil, fmt.Errorf("unsupported UniScalarValue kind %T", scalar.Inner)
	}
}

func nativeParams(args []any) ([]types.UniDataValue, error) {
	params := make([]types.UniDataValue, 0, len(args))
	for _, arg := range args {
		datum, err := nativeToDatum(arg)
		if err != nil {
			return nil, err
		}
		params = append(params, datum)
	}
	return params, nil
}
