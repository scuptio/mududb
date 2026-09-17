"""Canonical `mududb.result` segment: row-based query results.

Mirrors the AssemblyScript `ResultSet` / `Row` semantics exactly — the
cursor starts before the first row, `next()` advances it, and
`current_row()` is valid only after a successful `next()`. The host drains
all rows into the first query response, so iteration never goes back to
the wire.
"""

from mududb.errors import MuduError
from mududb.generated.uni_data_value import UniDataValue
from mududb.generated.uni_query_result import UniQueryResult
from mududb.generated.uni_scalar_value import (
    UniScalarValueBlob,
    UniScalarValueBool,
    UniScalarValueF64,
    UniScalarValueI64,
    UniScalarValueNull,
    UniScalarValueString,
)

__all__ = ["ResultSet", "Row", "as_i64", "as_f64", "as_bool", "as_string", "as_bytes"]


def _scalar(v: UniDataValue, what: str):
    inner = getattr(v, "inner", None)
    if inner is None:
        raise MuduError(1, f"value is not a scalar ({what} expected)", "mududb.result")
    return inner


def as_i64(v: UniDataValue) -> int:
    """The value as a Python int (accepts any integer scalar width)."""
    scalar = _scalar(v, "integer")
    if isinstance(scalar, UniScalarValueNull):
        raise MuduError(1, "value is NULL", "mududb.result")
    inner = getattr(scalar, "inner", None)
    if isinstance(inner, bool) or not isinstance(inner, (int, float)):
        raise MuduError(1, f"value is not an integer scalar: {type(scalar).__name__}", "mududb.result")
    return int(inner)


def as_f64(v: UniDataValue) -> float:
    scalar = _scalar(v, "float")
    if isinstance(scalar, UniScalarValueF64):
        return scalar.inner
    raise MuduError(1, f"value is not an f64 scalar: {type(scalar).__name__}", "mududb.result")


def as_bool(v: UniDataValue) -> bool:
    scalar = _scalar(v, "bool")
    if isinstance(scalar, UniScalarValueBool):
        return scalar.inner
    raise MuduError(1, f"value is not a bool scalar: {type(scalar).__name__}", "mududb.result")


def as_string(v: UniDataValue) -> str:
    scalar = _scalar(v, "string")
    if isinstance(scalar, UniScalarValueString):
        return scalar.inner
    raise MuduError(1, f"value is not a string scalar: {type(scalar).__name__}", "mududb.result")


def as_bytes(v: UniDataValue) -> bytes:
    scalar = _scalar(v, "bytes")
    if isinstance(scalar, UniScalarValueBlob):
        return scalar.inner
    raise MuduError(1, f"value is not a blob scalar: {type(scalar).__name__}", "mududb.result")


class ResultSet:
    """In-memory result set of a `query` call."""

    def __init__(self, result: UniQueryResult):
        self._rows = result.result_set.row_set
        self._columns = [f.field_name for f in result.tuple_desc.record_fields]
        # 0 means "before the first row"; after a successful next() it holds
        # the 1-based index of the current row.
        self._cursor = 0

    def next(self) -> bool:
        """Advance the cursor; False once the rows are exhausted."""
        if self._cursor < len(self._rows):
            self._cursor += 1
            return True
        return False

    def current_row(self) -> "Row":
        """The row under the cursor; an error before the first next() and
        after exhaustion."""
        if self._cursor == 0 or self._cursor > len(self._rows):
            raise MuduError(1, "no current row", "mududb.result")
        return Row(self._columns, self._rows[self._cursor - 1].fields)

    def column_count(self) -> int:
        return len(self._columns)

    def column_name(self, column: int) -> str:
        if column < 0 or column >= len(self._columns):
            raise MuduError(1, "column index is out of range", "mududb.result")
        return self._columns[column]

    def find_column(self, name: str) -> int:
        try:
            return self._columns.index(name)
        except ValueError:
            raise MuduError(1, f"column name not found: {name}", "mududb.result") from None

    def eof(self) -> bool:
        return self._cursor >= len(self._rows)


class Row:
    """A single row; values are reachable by column index or by name."""

    def __init__(self, columns: list[str], values: list[UniDataValue]):
        self._columns = columns
        self._values = values

    def is_null(self, column: int) -> bool:
        value = self.value(column)
        return isinstance(getattr(value, "inner", None), UniScalarValueNull)

    def is_null_by_name(self, name: str) -> bool:
        return self.is_null(self._find_column(name))

    def value(self, column: int) -> UniDataValue:
        if column < 0 or column >= len(self._values):
            raise MuduError(1, "column index is out of range", "mududb.result")
        return self._values[column]

    def value_by_name(self, name: str) -> UniDataValue:
        return self.value(self._find_column(name))

    def _find_column(self, name: str) -> int:
        try:
            return self._columns.index(name)
        except ValueError:
            raise MuduError(1, f"column name not found: {name}", "mududb.result") from None
