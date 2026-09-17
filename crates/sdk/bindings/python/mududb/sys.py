"""Low-level syscall transport (`mududb.sys`).

The transport is a table of byte-pipe callables. Inside a guest it is the
componentize-py-generated `wit_world.imports.system` module (plain
`func(list<u8>) -> list<u8>` functions); tests inject fakes. The transport
is installed lazily via `set_transport`, so importing `mududb` never
requires a host. `UniError` wire results surface as `MuduError`.
"""

from mududb.errors import MuduError
from mududb.generated import uni_syscall as _sc
from mududb.generated.uni_fs_dirent import UniFsDirent
from mududb.generated.uni_fs_open_argv import UniFsOpenArgv
from mududb.generated.uni_fs_stat import UniFsStat
from mududb.generated.uni_oid import UniOid

__all__ = [
    "set_transport",
    "parse_worker_oid",
    "wit_open",
    "wit_close",
    "wit_query_bytes",
    "wit_command_bytes",
    "wit_batch_bytes",
    "fs_open",
    "fs_close",
    "fs_read",
    "fs_write",
    "fs_pread",
    "fs_pwrite",
    "fs_lseek",
    "fs_fstat",
    "fs_stat",
    "fs_fsync",
    "fs_readdir",
]

_transport = None


def set_transport(transport) -> None:
    """Install the syscall transport: a module/object with the 23 byte-pipe
    functions as attributes (`open`, `query`, ..., `fs_readdir`), or a dict
    mapping those names to callables."""
    global _transport
    _transport = transport


def _call(name: str, request: bytes) -> bytes:
    if _transport is None:
        raise MuduError(
            1,
            "no syscall transport installed (the mududb facades only work "
            "inside a guest, or after set_transport in tests)",
            "mududb.sys",
        )
    if isinstance(_transport, dict):
        fn = _transport[name]
    else:
        fn = getattr(_transport, name)
    return fn(request)


def _raise_if_error(result) -> None:
    if result.error is not None:
        e = result.error
        raise MuduError(e.err_code, e.err_msg, e.err_src, e.err_loc)


def parse_worker_oid(uri: str) -> UniOid:
    """The open "uri" is either empty (the default worker) or a decimal u128
    worker object id (same rule as the AssemblyScript `parseWorkerOid`)."""
    if uri == "":
        return UniOid()
    if not uri.isdigit():
        raise MuduError(
            1, "open uri must be empty or a numeric worker object id", "mududb.sys"
        )
    value = int(uri)
    if value >= 1 << 128:
        raise MuduError(1, "worker object id does not fit into u128", "mududb.sys")
    return UniOid(h=value >> 64, l=value & 0xFFFFFFFFFFFFFFFF)


# ---- session ----


def wit_open(uri: str = "") -> UniOid:
    """Open a session; `uri` follows [`parse_worker_oid`] semantics."""
    result = _sc.decode_open_session_result(
        _call("open", _sc.encode_open_session_request(parse_worker_oid(uri)))
    )
    _raise_if_error(result)
    return result.value


def wit_close(session: UniOid) -> None:
    result = _sc.decode_close_session_result(
        _call("close", _sc.encode_close_session_request(session))
    )
    _raise_if_error(result)


# ---- SQL byte pipes (used by mududb.db) ----


def wit_query_bytes(request: bytes) -> bytes:
    return _call("query", request)


def wit_command_bytes(request: bytes) -> bytes:
    return _call("command", request)


def wit_batch_bytes(request: bytes) -> bytes:
    return _call("batch", request)


# ---- fs ----


def fs_open(session: UniOid, oid: UniOid, path: str, flags: int) -> int:
    argv = UniFsOpenArgv(session=session, oid=oid, path=path, flags=flags)
    result = _sc.decode_fs_open_result(_call("fs_open", _sc.encode_fs_open_request(argv)))
    _raise_if_error(result)
    return result.value


def fs_close(fd: int) -> None:
    result = _sc.decode_fs_close_result(_call("fs_close", _sc.encode_fs_close_request(fd)))
    _raise_if_error(result)


def fs_read(fd: int, len: int) -> bytes:
    result = _sc.decode_fs_read_result(_call("fs_read", _sc.encode_fs_read_request(fd, len)))
    _raise_if_error(result)
    return result.value


def fs_write(fd: int, data: bytes) -> int:
    result = _sc.decode_fs_write_result(
        _call("fs_write", _sc.encode_fs_write_request(fd, data))
    )
    _raise_if_error(result)
    return result.value


def fs_pread(fd: int, offset: int, len: int) -> bytes:
    result = _sc.decode_fs_pread_result(
        _call("fs_pread", _sc.encode_fs_pread_request(fd, offset, len))
    )
    _raise_if_error(result)
    return result.value


def fs_pwrite(fd: int, offset: int, data: bytes) -> None:
    result = _sc.decode_fs_pwrite_result(
        _call("fs_pwrite", _sc.encode_fs_pwrite_request(fd, offset, data))
    )
    _raise_if_error(result)


def fs_lseek(fd: int, offset: int, whence: int) -> int:
    result = _sc.decode_fs_lseek_result(
        _call("fs_lseek", _sc.encode_fs_lseek_request(fd, offset, whence))
    )
    _raise_if_error(result)
    return result.value


def fs_fstat(fd: int) -> UniFsStat:
    result = _sc.decode_fs_fstat_result(
        _call("fs_fstat", _sc.encode_fs_fstat_request(fd))
    )
    _raise_if_error(result)
    return result.value


def fs_stat(oid: UniOid, path: str) -> UniFsStat:
    result = _sc.decode_fs_stat_result(
        _call("fs_stat", _sc.encode_fs_stat_request(oid, path))
    )
    _raise_if_error(result)
    return result.value


def fs_fsync(fd: int) -> None:
    result = _sc.decode_fs_fsync_result(
        _call("fs_fsync", _sc.encode_fs_fsync_request(fd))
    )
    _raise_if_error(result)


def fs_readdir(oid: UniOid, path: str) -> list[UniFsDirent]:
    result = _sc.decode_fs_readdir_result(
        _call("fs_readdir", _sc.encode_fs_readdir_request(oid, path))
    )
    _raise_if_error(result)
    return result.value
