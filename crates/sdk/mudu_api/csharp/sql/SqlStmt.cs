#nullable enable

namespace mududb.sql;

/// <summary>
/// Canonical facade `mududb.sql`: the SQL statement text, mirroring the
/// AssemblyScript `SqlStmt`. Immutable; the statement keeps its `?` / `:name`
/// placeholders and the values travel separately in a <see cref="SqlParams"/>.
/// </summary>
public sealed class SqlStmt
{
    /// <summary>
    /// Wraps the SQL statement text.
    /// </summary>
    public SqlStmt(string sql)
    {
        Sql = sql ?? throw new global::System.ArgumentNullException(nameof(sql));
    }

    /// <summary>
    /// The statement text, placeholders included.
    /// </summary>
    public string Sql { get; }
}
