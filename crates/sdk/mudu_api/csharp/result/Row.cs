#nullable enable

using mududb.types;

namespace mududb.result;

/// <summary>
/// Canonical facade `mududb.result`: a single row of a
/// <see cref="ResultSet"/>; values are reachable by column index or by column
/// name, mirroring the AssemblyScript `Row` semantics exactly.
/// </summary>
public sealed class Row
{
    private readonly global::System.Collections.Generic.IReadOnlyList<string> columns;
    private readonly global::System.Collections.Generic.IReadOnlyList<UniDataValue> values;

    internal Row(
        global::System.Collections.Generic.IReadOnlyList<string> columns,
        global::System.Collections.Generic.IReadOnlyList<UniDataValue> values)
    {
        this.columns = columns;
        this.values = values;
    }

    /// <summary>
    /// The value at `column` (0-based); throws
    /// <see cref="global::System.ArgumentOutOfRangeException"/> when out of
    /// range.
    /// </summary>
    public UniDataValue Value(int column)
    {
        if (column < 0 || column >= values.Count)
        {
            throw new global::System.ArgumentOutOfRangeException(nameof(column), "column index is out of range");
        }

        return values[column];
    }

    /// <summary>
    /// The value in the column called `name`; throws
    /// <see cref="global::System.ArgumentException"/> when it does not exist.
    /// </summary>
    public UniDataValue ValueByName(string name)
    {
        return Value(FindColumn(name));
    }

    /// <summary>
    /// Whether the value at `column` (0-based) is NULL; throws
    /// <see cref="global::System.ArgumentOutOfRangeException"/> when out of
    /// range.
    /// </summary>
    public bool IsNull(int column)
    {
        return IsNullValue(Value(column));
    }

    /// <summary>
    /// Whether the value in the column called `name` is NULL; throws
    /// <see cref="global::System.ArgumentException"/> when the column does
    /// not exist.
    /// </summary>
    public bool IsNullByName(string name)
    {
        return IsNull(FindColumn(name));
    }

    private static bool IsNullValue(UniDataValue value)
    {
        return value.Kind() == UniDataValueKind.Scalar
            && UniDataValueScalar.AsScalar(value).Inner.Kind() == UniScalarValueKind.Null;
    }

    private int FindColumn(string name)
    {
        for (var i = 0; i < columns.Count; i++)
        {
            if (columns[i] == name)
            {
                return i;
            }
        }

        throw new global::System.ArgumentException($"column name not found: {name}", nameof(name));
    }
}
