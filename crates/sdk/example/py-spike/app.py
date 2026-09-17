"""Minimal mududb Python guest for the componentize-py spike.

Exports `mp2-hello` (the mp2 byte-pipe shape): decodes the UniProcedureParam
argument, does an open/close session roundtrip through the host's
`mududb:api/system` imports, and returns a text result through the
canonical `{0: UniProcedureResult}` encoding.
"""

import wit_world
from wit_world.imports import system

from mududb.codec.mpack import MpackReader, MpackWriter
from mududb.generated.uni_data_value import UniDataValueScalar
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_procedure_param import uni_procedure_param_from_value
from mududb.generated.uni_procedure_result import (
    UniProcedureResult,
    uni_procedure_result_to_value,
)
from mududb.generated.uni_scalar_value import UniScalarValueString
from mududb.generated.uni_syscall import (
    decode_close_session_result,
    decode_open_session_result,
    encode_close_session_request,
    encode_open_session_request,
)


def _ok_payload(text: str) -> bytes:
    result = UniProcedureResult(
        return_list=[UniDataValueScalar(inner=UniScalarValueString(inner=text))]
    )
    writer = MpackWriter()
    writer.write_map_header(1)
    writer.write_u64(0)
    writer.write_value(uni_procedure_result_to_value(result))
    return writer.to_bytes()


class WitWorld(wit_world.WitWorld):
    def mp2_hello(self, param: bytes) -> bytes:
        reader = MpackReader(param)
        proc_param = uni_procedure_param_from_value(reader.read_value())

        opened = decode_open_session_result(
            system.open(encode_open_session_request(UniOid()))
        )
        if opened.error is not None:
            status = f"failed: {opened.error.err_msg}"
        else:
            session = opened.value
            closed = decode_close_session_result(
                system.close(encode_close_session_request(session))
            )
            status = (
                "ok" if closed.error is None else f"failed: {closed.error.err_msg}"
            )

        return _ok_payload(
            f"hello-from-py params:{len(proc_param.param_list)} open_close:{status}"
        )
