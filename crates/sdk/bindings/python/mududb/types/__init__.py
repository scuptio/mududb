"""Facade re-exporting the ``Uni*`` record/variant types from the
mgen-generated modules in :mod:`mududb.generated`. The ``uni_syscall``
frame codec module is not a type module; import it via ``mududb.codec``
or ``mududb.generated.uni_syscall`` instead."""

from mududb.generated.uni_command_argv import UniCommandArgv
from mududb.generated.uni_command_result import (
    UniCommandResult,
    UniCommandReturn,
    UniCommandReturnErr,
    UniCommandReturnKind,
    UniCommandReturnOk,
)
from mududb.generated.uni_data_type import (
    UniDataType,
    UniDataTypeArray,
    UniDataTypeBinary,
    UniDataTypeBox,
    UniDataTypeIdentifier,
    UniDataTypeKind,
    UniDataTypeOption,
    UniDataTypeRecord,
    UniDataTypeResult,
    UniDataTypeScalar,
    UniDataTypeTuple,
)
from mududb.generated.uni_data_value import (
    UniDataValue,
    UniDataValueArray,
    UniDataValueBinary,
    UniDataValueField,
    UniDataValueKind,
    UniDataValueRecord,
    UniDataValueScalar,
)
from mududb.generated.uni_error import UniError
from mududb.generated.uni_fs_dirent import UniFsDirent
from mududb.generated.uni_fs_open_argv import UniFsOpenArgv
from mududb.generated.uni_fs_stat import UniFsStat
from mududb.generated.uni_message import UniMessage
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_procedure_param import UniProcedureParam
from mududb.generated.uni_procedure_result import UniProcedureResult
from mududb.generated.uni_query_argv import UniQueryArgv
from mududb.generated.uni_query_result import (
    UniQueryResult,
    UniQueryReturn,
    UniQueryReturnErr,
    UniQueryReturnKind,
    UniQueryReturnOk,
)
from mududb.generated.uni_record_type import (
    UniFieldAttr,
    UniRecordField,
    UniRecordType,
)
from mududb.generated.uni_result_set import UniResultSet
from mududb.generated.uni_result_type import UniResultType
from mududb.generated.uni_scalar import UniScalar
from mududb.generated.uni_scalar_value import (
    UniScalarValue,
    UniScalarValueBlob,
    UniScalarValueBool,
    UniScalarValueChar,
    UniScalarValueDate,
    UniScalarValueF32,
    UniScalarValueF64,
    UniScalarValueI8,
    UniScalarValueI16,
    UniScalarValueI32,
    UniScalarValueI64,
    UniScalarValueI128,
    UniScalarValueKind,
    UniScalarValueNull,
    UniScalarValueNumeric,
    UniScalarValueString,
    UniScalarValueTime,
    UniScalarValueTimestamp,
    UniScalarValueTimestampTz,
    UniScalarValueU8,
    UniScalarValueU16,
    UniScalarValueU32,
    UniScalarValueU64,
    UniScalarValueU128,
)
from mududb.generated.uni_sql_param import UniSqlParam
from mududb.generated.uni_sql_stmt import UniSqlStmt
from mududb.generated.uni_tuple_row import UniTupleRow

__all__ = [
    "UniCommandArgv",
    "UniCommandResult",
    "UniCommandReturn",
    "UniCommandReturnErr",
    "UniCommandReturnKind",
    "UniCommandReturnOk",
    "UniDataType",
    "UniDataTypeArray",
    "UniDataTypeBinary",
    "UniDataTypeBox",
    "UniDataTypeIdentifier",
    "UniDataTypeKind",
    "UniDataTypeOption",
    "UniDataTypeRecord",
    "UniDataTypeResult",
    "UniDataTypeScalar",
    "UniDataTypeTuple",
    "UniDataValue",
    "UniDataValueArray",
    "UniDataValueBinary",
    "UniDataValueField",
    "UniDataValueKind",
    "UniDataValueRecord",
    "UniDataValueScalar",
    "UniError",
    "UniFieldAttr",
    "UniFsDirent",
    "UniFsOpenArgv",
    "UniFsStat",
    "UniMessage",
    "UniOid",
    "UniProcedureParam",
    "UniProcedureResult",
    "UniQueryArgv",
    "UniQueryResult",
    "UniQueryReturn",
    "UniQueryReturnErr",
    "UniQueryReturnKind",
    "UniQueryReturnOk",
    "UniRecordField",
    "UniRecordType",
    "UniResultSet",
    "UniResultType",
    "UniScalar",
    "UniScalarValue",
    "UniScalarValueBlob",
    "UniScalarValueBool",
    "UniScalarValueChar",
    "UniScalarValueDate",
    "UniScalarValueF32",
    "UniScalarValueF64",
    "UniScalarValueI8",
    "UniScalarValueI16",
    "UniScalarValueI32",
    "UniScalarValueI64",
    "UniScalarValueI128",
    "UniScalarValueKind",
    "UniScalarValueNull",
    "UniScalarValueNumeric",
    "UniScalarValueString",
    "UniScalarValueTime",
    "UniScalarValueTimestamp",
    "UniScalarValueTimestampTz",
    "UniScalarValueU8",
    "UniScalarValueU16",
    "UniScalarValueU32",
    "UniScalarValueU64",
    "UniScalarValueU128",
    "UniSqlParam",
    "UniSqlStmt",
    "UniTupleRow",
]
