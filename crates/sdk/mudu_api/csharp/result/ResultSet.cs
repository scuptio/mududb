#nullable enable

namespace mududb.result;

/// <summary>
/// Canonical facade `mududb.result`: an in-memory row set, mirroring the
/// AssemblyScript `ResultSet` semantics exactly. The host drains all rows
/// into the first query response, so iteration never goes back to the wire.
/// The cursor starts before the first row, <see cref="Next"/> advances it,
/// and <see cref="CurrentRow"/> is valid only after a successful
/// <see cref="Next"/>.
/// </summary>
public sealed class ResultSet
{
    private readonly global::System.Collections.Generic.List<string> columns;
    private readonly global::System.Collections.Generic.List<UniTupleRow> rows;

    /// <summary>
    /// 0 means "before the first row"; after a successful <see cref="Next"/>
    /// it holds the 1-based index of the current row.
    /// </summary>
    private int cursor;

    /// <summary>
    /// Wraps the decoded `UniQueryResult`: column names from
    /// `TupleDesc.RecordFields[].FieldName`, rows from `ResultSet.RowSet`.
    /// </summary>
    public ResultSet(UniQueryResult result)
    {
        rows = result.ResultSet.RowSet;
        cursor = 0;
        var fields = result.TupleDesc.RecordFields;
        columns = new global::System.Collections.Generic.List<string>(fields.Count);
        foreach (var field in fields)
        {
            columns.Add(field.FieldName);
        }
    }

    /// <summary>
    /// Advance the cursor; returns `false` once the rows are exhausted.
    /// </summary>
    public bool Next()
    {
        if (cursor < rows.Count)
        {
            cursor += 1;
            return true;
        }

        return false;
    }

    /// <summary>
    /// The row under the cursor; throws
    /// <see cref="global::System.InvalidOperationException"/> before the first
    /// <see cref="Next"/> and after the rows are exhausted.
    /// </summary>
    public Row CurrentRow()
    {
        if (cursor == 0 || cursor > rows.Count)
        {
            throw new global::System.InvalidOperationException("no current row");
        }

        return new Row(columns, rows[cursor - 1].Fields);
    }

    /// <summary>
    /// Number of columns in every row.
    /// </summary>
    public int ColumnCount()
    {
        return columns.Count;
    }

    /// <summary>
    /// Name of the `column`-th column (0-based); throws
    /// <see cref="global::System.ArgumentOutOfRangeException"/> when out of
    /// range.
    /// </summary>
    public string ColumnName(int column)
    {
        if (column < 0 || column >= columns.Count)
        {
            throw new global::System.ArgumentOutOfRangeException(nameof(column), "column index is out of range");
        }

        return columns[column];
    }

    /// <summary>
    /// Index of the column called `name`; throws
    /// <see cref="global::System.ArgumentException"/> when it does not exist.
    /// </summary>
    public int FindColumn(string name)
    {
        var index = columns.IndexOf(name);
        if (index < 0)
        {
            throw new global::System.ArgumentException($"column name not found: {name}", nameof(name));
        }

        return index;
    }

    /// <summary>
    /// Whether the cursor has passed the last row.
    /// </summary>
    public bool Eof()
    {
        return cursor >= rows.Count;
    }
}
