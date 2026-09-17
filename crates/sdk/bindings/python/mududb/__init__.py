"""mududb Python bindings.

Canonical import paths:

- ``mududb.codec.mpack`` — the handwritten MessagePack runtime
  (:class:`F32`, :class:`MpackReader`, :class:`MpackWriter`).
- ``mududb.generated`` — the mgen-generated MSSP codecs (``uni_*`` modules,
  including ``uni_syscall`` with the per-message-kind frame codecs).
- ``mududb.types`` — facade re-exporting the ``Uni*`` record/variant types
  from ``mududb.generated``.
- ``mududb.db`` / ``mududb.sql`` / ``mududb.result`` / ``mududb.fs`` /
  ``mududb.sys`` — the canonical guest facade (see
  ``doc/dev/binding_api_surface.md``); usable inside a guest, or with an
  injected transport in tests.
"""

from mududb import codec, db, errors, fs, generated, result, sql, sys, types
from mududb.codec.mpack import F32, MpackReader, MpackWriter

__all__ = [
    "F32",
    "MpackReader",
    "MpackWriter",
    "codec",
    "db",
    "errors",
    "fs",
    "generated",
    "result",
    "sql",
    "sys",
    "types",
]
