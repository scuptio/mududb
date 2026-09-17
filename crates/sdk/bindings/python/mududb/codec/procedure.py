"""mp2 byte-pipe helpers for procedure adapters (`mududb.codec.procedure`).

Decodes the `UniProcedureParam` the runtime passes to each `mp2-<proc>`
export and encodes the `UniResult<UniProcedureResult, UniError>` it expects
back (a single-entry MessagePack map: key 0 = ok, key 1 = `UniError`).
Mirrors the AssemblyScript `procedure.ts` helpers exactly.
"""

from mududb.codec.mpack import MpackReader, MpackWriter
from mududb.generated.uni_data_value import UniDataValue
from mududb.generated.uni_error import UniError, uni_error_to_value
from mududb.generated.uni_procedure_param import (
    UniProcedureParam,
    uni_procedure_param_from_value,
)
from mududb.generated.uni_procedure_result import (
    UniProcedureResult,
    uni_procedure_result_to_value,
)

__all__ = [
    "decode_procedure_param",
    "encode_procedure_ok",
    "encode_procedure_err",
]

ERROR_INTERNAL = 1


def decode_procedure_param(param: bytes) -> UniProcedureParam:
    """Decode the canonical-ABI argument bytes of an `mp2-<proc>` export."""
    reader = MpackReader(param)
    return uni_procedure_param_from_value(reader.read_value())


def _encode_arm(key: int, value) -> bytes:
    writer = MpackWriter()
    writer.write_map_header(1)
    writer.write_u64(key)
    writer.write_value(value)
    return writer.to_bytes()


def encode_procedure_ok(values: list[UniDataValue]) -> bytes:
    """Ok arm: `{0: {1: [return_list]}}`."""
    return _encode_arm(
        0,
        uni_procedure_result_to_value(UniProcedureResult(return_list=values)),
    )


def encode_procedure_err(
    code: int, message: str, source: str = "python", location: str = ""
) -> bytes:
    """Err arm: `{1: UniError}`; the error source defaults to "python"."""
    error = UniError(
        err_code=code if code != 0 else ERROR_INTERNAL,
        err_msg=message,
        err_src=source,
        err_loc=location,
    )
    return _encode_arm(1, uni_error_to_value(error))
