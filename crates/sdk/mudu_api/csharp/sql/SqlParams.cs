#nullable enable

using mududb.types;

namespace mududb.sql;

/// <summary>
/// Canonical facade `mududb.sql`: the parameter list of a
/// <see cref="SqlStmt"/>, mirroring the AssemblyScript `ValueList` exactly.
/// Positional `?` placeholders are bound with <see cref="Bind(int, UniDataValue?)"/>
/// (indices contiguous from 0, no re-binding) <b>or</b> `:name` placeholders
/// with <see cref="BindNamed(string, UniDataValue?)"/>; the two styles cannot
/// be mixed in one list. Named parameters are resolved host-side: the
/// statement text keeps the `:name` placeholders and a repeated `:name`
/// reuses its value.
/// </summary>
public sealed class SqlParams
{
    private readonly global::System.Collections.Generic.List<UniDataValue> items = [];
    private global::System.Collections.Generic.List<string>? names;

    /// <summary>
    /// Bind `value` to the positional `?` placeholder `index`.
    /// </summary>
    /// <remarks>
    /// Indices must be contiguous from 0 (bind 0..n-1 for n placeholders);
    /// gaps, negative indices, and re-binding the same index are rejected
    /// with <see cref="global::System.ArgumentException"/>. A null `value`
    /// binds SQL NULL.
    /// </remarks>
    public SqlParams Bind(int index, UniDataValue? value)
    {
        if (names is not null)
        {
            throw new global::System.ArgumentException("positional bind cannot be mixed with bindNamed", nameof(index));
        }

        if (index != items.Count)
        {
            throw new global::System.ArgumentException("bind indices must be contiguous from 0", nameof(index));
        }

        items.Add(value ?? NullValue());
        return this;
    }

    /// <summary>
    /// Bind `value` to the named placeholder `:name` in the SQL statement.
    /// </summary>
    /// <remarks>
    /// Indexed (<see cref="Bind(int, UniDataValue?)"/>) and named values
    /// cannot be mixed in one list; violations are rejected with
    /// <see cref="global::System.ArgumentException"/>. A null `value` binds
    /// SQL NULL.
    /// </remarks>
    public SqlParams BindNamed(string name, UniDataValue? value)
    {
        if (names is null)
        {
            if (items.Count > 0)
            {
                throw new global::System.ArgumentException("bindNamed cannot be mixed with positional bind", nameof(name));
            }

            names = [];
        }

        items.Add(value ?? NullValue());
        names.Add(name ?? throw new global::System.ArgumentNullException(nameof(name)));
        return this;
    }

    /// <summary>
    /// Bind a 32-bit integer to the positional `?` placeholder `index`.
    /// </summary>
    public SqlParams Bind(int index, int value)
    {
        return Bind(index, Scalar(new UniScalarValueI32 { Inner = value }));
    }

    /// <summary>
    /// Bind a 64-bit integer to the positional `?` placeholder `index`.
    /// </summary>
    public SqlParams Bind(int index, long value)
    {
        return Bind(index, Scalar(new UniScalarValueI64 { Inner = value }));
    }

    /// <summary>
    /// Bind a boolean to the positional `?` placeholder `index`.
    /// </summary>
    public SqlParams Bind(int index, bool value)
    {
        return Bind(index, Scalar(new UniScalarValueBool { Inner = value }));
    }

    /// <summary>
    /// Bind a 64-bit float to the positional `?` placeholder `index`.
    /// </summary>
    public SqlParams Bind(int index, double value)
    {
        return Bind(index, Scalar(new UniScalarValueF64 { Inner = value }));
    }

    /// <summary>
    /// Bind a common CLR value to the positional `?` placeholder `index`:
    /// null (SQL NULL), <see cref="int"/>, <see cref="long"/>,
    /// <see cref="string"/>, <see cref="bool"/>, <see cref="double"/>,
    /// <see cref="byte"/>[] (scalar blob, the same wire case the
    /// AssemblyScript guest uses), or an already-built
    /// <see cref="UniDataValue"/> / <see cref="UniScalarValue"/>. Any other
    /// type is rejected with <see cref="global::System.ArgumentException"/>.
    /// </summary>
    public SqlParams Bind(int index, object? value)
    {
        return Bind(index, ToUniDataValue(value));
    }

    /// <summary>
    /// Bind a 32-bit integer to the named placeholder `:name`.
    /// </summary>
    public SqlParams BindNamed(string name, int value)
    {
        return BindNamed(name, Scalar(new UniScalarValueI32 { Inner = value }));
    }

    /// <summary>
    /// Bind a 64-bit integer to the named placeholder `:name`.
    /// </summary>
    public SqlParams BindNamed(string name, long value)
    {
        return BindNamed(name, Scalar(new UniScalarValueI64 { Inner = value }));
    }

    /// <summary>
    /// Bind a boolean to the named placeholder `:name`.
    /// </summary>
    public SqlParams BindNamed(string name, bool value)
    {
        return BindNamed(name, Scalar(new UniScalarValueBool { Inner = value }));
    }

    /// <summary>
    /// Bind a 64-bit float to the named placeholder `:name`.
    /// </summary>
    public SqlParams BindNamed(string name, double value)
    {
        return BindNamed(name, Scalar(new UniScalarValueF64 { Inner = value }));
    }

    /// <summary>
    /// Bind a common CLR value to the named placeholder `:name`; the accepted
    /// types are the same as <see cref="Bind(int, object?)"/>.
    /// </summary>
    public SqlParams BindNamed(string name, object? value)
    {
        return BindNamed(name, ToUniDataValue(value));
    }

    /// <summary>
    /// Wire form for query/command/batch argv: positional values under the
    /// `params` key (always present, possibly empty); the `param-names` key
    /// is present only for named binding.
    /// </summary>
    public UniSqlParam ToUniSqlParam()
    {
        return new UniSqlParam
        {
            Params = [.. items],
            ParamNames = names is null ? null : [.. names],
        };
    }

    private static UniDataValue ToUniDataValue(object? value)
    {
        return value switch
        {
            null => NullValue(),
            UniDataValue dataValue => dataValue,
            UniScalarValue scalarValue => Scalar(scalarValue),
            int i32 => Scalar(new UniScalarValueI32 { Inner = i32 }),
            long i64 => Scalar(new UniScalarValueI64 { Inner = i64 }),
            string text => Scalar(new UniScalarValueString { Inner = text }),
            bool boolean => Scalar(new UniScalarValueBool { Inner = boolean }),
            double f64 => Scalar(new UniScalarValueF64 { Inner = f64 }),
            byte[] blob => Scalar(new UniScalarValueBlob { Inner = [.. blob] }),
            _ => throw new global::System.ArgumentException(
                $"unsupported parameter value type: {value.GetType().Name}", nameof(value)),
        };
    }

    private static UniDataValue NullValue()
    {
        return Scalar(new UniScalarValueNull());
    }

    private static UniDataValue Scalar(UniScalarValue scalar)
    {
        return new UniDataValueScalar { Inner = scalar };
    }
}
