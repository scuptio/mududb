"""Optional native mudu_adapter transport (ABI v1); separate runtime required."""
import ctypes
import os
from pathlib import Path

class _Buffer(ctypes.Structure):
    _fields_ = [("data", ctypes.POINTER(ctypes.c_ubyte)), ("len", ctypes.c_size_t)]

class AdapterTransport:
    """Load an explicitly selected native adapter. Configure MUDU_CONNECTION first."""
    def __init__(self, library=None):
        library = library or os.environ.get("MUDU_ADAPTER_LIBRARY")
        if not library:
            raise RuntimeError("Set MUDU_ADAPTER_LIBRARY to the extracted adapter runtime library")
        try:
            self._lib = ctypes.CDLL(str(Path(library).resolve(strict=True)))
        except (OSError, ValueError) as exc:
            raise RuntimeError("Cannot load adapter: check library path, OS, architecture and dependencies") from exc
        self._lib.mudu_adapter_abi_version.restype = ctypes.c_uint32
        if self._lib.mudu_adapter_abi_version() != 1:
            raise RuntimeError("Incompatible adapter ABI: install an ABI v1 runtime")
        self._lib.mudu_adapter_call.argtypes = [ctypes.c_char_p, ctypes.c_size_t,
            ctypes.c_char_p, ctypes.c_size_t, ctypes.POINTER(_Buffer)]
        self._lib.mudu_adapter_call.restype = ctypes.c_int32
        self._lib.mudu_adapter_free.argtypes = [_Buffer]
        self._lib.mudu_adapter_free.restype = None

    def call(self, operation, frame):
        """Send one MSSP frame, copying and releasing the library-owned result."""
        operation = operation.encode("ascii")
        frame = bytes(frame)
        out = _Buffer()
        status = self._lib.mudu_adapter_call(operation, len(operation), frame, len(frame), ctypes.byref(out))
        try:
            result = ctypes.string_at(out.data, out.len) if out.len else b""
        finally:
            self._lib.mudu_adapter_free(out)
        if status:
            raise RuntimeError(result.decode("utf-8", errors="replace") or f"Adapter ABI error {status}")
        return result

    def __getattr__(self, operation):
        if operation.startswith("_"):
            raise AttributeError(operation)
        return lambda frame: self.call(operation, frame)

def use_adapter(library=None):
    """Select the real adapter backend for mududb.db; does not enable a mock."""
    from mududb.sys import set_transport
    transport = AdapterTransport(library)
    set_transport(transport)
    return transport
