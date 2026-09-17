"""Canonical error type of the mududb Python bindings (`mududb.errors`).

Mirrors the AssemblyScript `MuduError` / the wire `UniError`: a numeric
`code`, a human-readable `message`, the `source` subsystem, and a `location`
(typically the procedure name).
"""

__all__ = ["MuduError"]


class MuduError(Exception):
    """An error raised by mududb facades and decoded from wire error frames."""

    def __init__(self, code: int, message: str, source: str = "", location: str = ""):
        super().__init__(message)
        self.code = code
        self.message = message
        self.source = source
        self.location = location

    def __str__(self) -> str:
        parts = self.message
        if self.source:
            parts = f"[{self.source}] {parts}"
        if self.location:
            parts = f"{parts} (at {self.location})"
        return parts
