"""Canonical `mududb.sql` segment: statements and parameter lists.

Mirrors the AssemblyScript `SqlStmt` / `ValueList` semantics exactly:
positional `bind` (indices contiguous from 0, no re-binding) **or** named
`bind_named` (`:name` placeholders, resolved host-side) — the two cannot be
mixed in one list.
"""

from mududb.errors import MuduError
from mududb.generated.uni_data_value import UniDataValue, UniDataValueScalar
from mududb.generated.uni_scalar_value import (
    UniScalarValueBlob,
    UniScalarValueBool,
    UniScalarValueF64,
    UniScalarValueI64,
    UniScalarValueNull,
    UniScalarValueString,
)
from mududb.generated.uni_sql_param import UniSqlParam

__all__ = ["SqlStmt", "Params", "to_uni_data_value"]


class SqlStmt:
    """A SQL statement text."""

    def __init__(self, sql: str):
        self.sql = sql


def to_uni_data_value(value) -> UniDataValue:
    """Wrap a plain Python value into its `UniDataValue` form: `bool` → Bool,
    `int` → I64, `float` → F64, `str` → String, `bytes` → Blob,
    `None` → Null; `UniDataValue` instances pass through."""
    if isinstance(value, UniDataValue):
        return value
    if value is None:
        scalar = UniScalarValueNull()
    elif isinstance(value, bool):
        scalar = UniScalarValueBool(inner=value)
    elif isinstance(value, int):
        scalar = UniScalarValueI64(inner=value)
    elif isinstance(value, float):
        scalar = UniScalarValueF64(inner=value)
    elif isinstance(value, str):
        scalar = UniScalarValueString(inner=value)
    elif isinstance(value, (bytes, bytearray)):
        scalar = UniScalarValueBlob(inner=bytes(value))
    else:
        raise MuduError(1, f"cannot bind value of type {type(value).__name__}", "mududb.sql")
    return UniDataValueScalar(inner=scalar)


class Params:
    """A parameter list for [`SqlStmt`]; see the module docstring for the
    bind rules."""

    def __init__(self):
        self._values: list[UniDataValue] = []
        self._names: list[str] | None = None

    def bind(self, index: int, value) -> "Params":
        """Bind `value` to the positional `?` placeholder `index` (indices
        must be contiguous from 0; gaps, negatives, and re-binding are
        rejected)."""
        if self._names is not None:
            raise MuduError(1, "positional bind cannot be mixed with bind_named", "mududb.sql")
        if index != len(self._values):
            raise MuduError(1, "bind indices must be contiguous from 0", "mududb.sql")
        self._values.append(to_uni_data_value(value))
        return self

    def bind_named(self, name: str, value) -> "Params":
        """Bind `value` to the named placeholder `:name` (resolved host-side;
        a repeated `:name` reuses the value)."""
        if self._names is None:
            if self._values:
                raise MuduError(1, "bind_named cannot be mixed with positional bind", "mududb.sql")
            self._names = []
        self._values.append(to_uni_data_value(value))
        self._names.append(name)
        return self

    def __len__(self) -> int:
        return len(self._values)

    def to_uni_sql_param(self) -> UniSqlParam:
        """Wire form: positional values under `params`; `param_names` present
        only for named binding."""
        return UniSqlParam(
            params=list(self._values),
            param_names=list(self._names) if self._names is not None else None,
        )
