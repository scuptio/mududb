"""Canonical `mududb.db` segment: the database session handle.

Mirrors the AssemblyScript `Database` semantics: `open`/`close`/`query`/
`command`/`batch`. Errors surface as `MuduError`.
"""

from mududb import sys as _sys
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_query_argv import UniQueryArgv
from mududb.generated.uni_command_argv import UniCommandArgv
from mududb.generated.uni_sql_stmt import UniSqlStmt as _UniSqlStmt
from mududb.generated import uni_syscall as _sc
from mududb.result import ResultSet
from mududb.sql import Params, SqlStmt

__all__ = ["Database"]


class Database:
    """A database session handle; obtain one with [`Database.open`]."""

    def __init__(self, id: UniOid):
        self.id = id

    @staticmethod
    def open(uri: str = "") -> "Database":
        """Open a session; `uri` is empty (default worker) or a decimal u128
        worker object id (same rule as the AssemblyScript `Database.open`)."""
        return Database(_sys.wit_open(uri))

    def close(self) -> None:
        _sys.wit_close(self.id)

    def query(self, stmt: SqlStmt, values: Params | None = None) -> ResultSet:
        """Run a SELECT statement and return the full result set."""
        argv = self._sql_argv(UniQueryArgv(), stmt, values)
        result = _sc.decode_query_result(
            _sys.wit_query_bytes(_sc.encode_query_request(argv))
        )
        _sys._raise_if_error(result)
        return ResultSet(result.value)

    def command(self, stmt: SqlStmt, values: Params | None = None) -> int:
        """Run an INSERT/UPDATE/DELETE; returns the affected row count."""
        return self._invoke_command(stmt, values, batch=False)

    def batch(self, stmt: SqlStmt, values: Params | None = None) -> int:
        """Batch path; same argument and return shape as [`Database.command`]."""
        return self._invoke_command(stmt, values, batch=True)

    def _invoke_command(self, stmt: SqlStmt, values: Params | None, batch: bool) -> int:
        argv = self._sql_argv(UniCommandArgv(), stmt, values)
        if batch:
            result = _sc.decode_batch_result(
                _sys.wit_batch_bytes(_sc.encode_batch_request(argv))
            )
        else:
            result = _sc.decode_command_result(
                _sys.wit_command_bytes(_sc.encode_command_request(argv))
            )
        _sys._raise_if_error(result)
        return result.value.affected_rows

    def _sql_argv(self, argv, stmt: SqlStmt, values: Params | None):
        argv.oid = self.id
        sql = _UniSqlStmt()
        sql.sql_string = stmt.sql
        if isinstance(argv, UniQueryArgv):
            argv.query = sql
        else:
            argv.command = sql
        # `param_desc` stays None: the host skips the type check then (same
        # as the other guests).
        argv.param_list = (
            values.to_uni_sql_param() if values is not None else Params().to_uni_sql_param()
        )
        return argv
