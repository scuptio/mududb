#nullable enable

using System.Data;
using MessagePack;
using Microsoft.Data.Sqlite;

namespace Mudu.Api.Mock;

public static class MockSqliteMuduSysCall
{
    public static string DatabasePath { get; set; } =
        global::System.Environment.GetEnvironmentVariable("MUDU_MOCK_SQLITE_PATH")
        ?? global::System.IO.Path.Combine(global::System.AppContext.BaseDirectory, "mudu_mock.db");

    public static byte[] QueryRaw(byte[] queryIn)
    {
        var argv = MessagePackSerializer.Deserialize<UniQueryArgv>(queryIn);
        var result = SysQuery(argv);
        return MessagePackSerializer.Serialize(result);
    }

    public static byte[] QueryRaw(global::System.ReadOnlyMemory<byte> queryIn)
    {
        return QueryRaw(queryIn.ToArray());
    }

    public static byte[] CommandRaw(byte[] commandIn)
    {
        var argv = MessagePackSerializer.Deserialize<UniCommandArgv>(commandIn);
        var result = SysCommand(argv);
        return MessagePackSerializer.Serialize(result);
    }

    public static byte[] CommandRaw(global::System.ReadOnlyMemory<byte> commandIn)
    {
        return CommandRaw(commandIn.ToArray());
    }

    public static byte[] FetchRaw(byte[] queryResult)
    {
        return queryResult;
    }

    public static byte[] FetchRaw(global::System.ReadOnlyMemory<byte> queryResult)
    {
        return queryResult.ToArray();
    }

    public static UniCommandReturn SysCommand(UniCommandArgv argv, MessagePackSerializerOptions? _options = null)
    {
        try
        {
            using var connection = OpenConnection();
            using var command = CreateCommand(connection, argv.Command.SqlString, argv.ParamList.Params);
            var affectedRows = command.ExecuteNonQuery();
            return new UniCommandReturnOk
            {
                Inner = new UniCommandResult
                {
                    AffectedRows = (ulong)global::System.Math.Max(affectedRows, 0),
                }
            };
        }
        catch (global::System.Exception ex)
        {
            return new UniCommandReturnErr
            {
                Inner = ToUniError(ex)
            };
        }
    }

    public static UniQueryReturn SysQuery(UniQueryArgv argv, MessagePackSerializerOptions? _options = null)
    {
        try
        {
            using var connection = OpenConnection();
            using var command = CreateCommand(connection, argv.Query.SqlString, argv.ParamList.Params);
            using var reader = command.ExecuteReader();

            var tupleDesc = BuildTupleDesc(reader);
            var rowSet = new global::System.Collections.Generic.List<UniTupleRow>();
            while (reader.Read())
            {
                rowSet.Add(ReadRow(reader));
            }

            return new UniQueryReturnOk
            {
                Inner = new UniQueryResult
                {
                    TupleDesc = tupleDesc,
                    ResultSet = new UniResultSet
                    {
                        Eof = true,
                        RowSet = rowSet,
                        Cursor = [],
                    }
                }
            };
        }
        catch (global::System.Exception ex)
        {
            return new UniQueryReturnErr
            {
                Inner = ToUniError(ex)
            };
        }
    }

    private static SqliteConnection OpenConnection()
    {
        var fullPath = global::System.IO.Path.GetFullPath(DatabasePath);
        var directory = global::System.IO.Path.GetDirectoryName(fullPath);
        if (!string.IsNullOrEmpty(directory))
        {
            global::System.IO.Directory.CreateDirectory(directory);
        }

        var connection = new SqliteConnection(new SqliteConnectionStringBuilder
        {
            DataSource = fullPath,
        }.ToString());
        connection.Open();
        return connection;
    }

    private static SqliteCommand CreateCommand(
        SqliteConnection connection,
        string sql,
        global::System.Collections.Generic.List<UniDatValue>? parameters)
    {
        var rewrittenSql = RewritePositionalParameters(sql, parameters?.Count ?? 0);
        var command = connection.CreateCommand();
        command.CommandText = rewrittenSql;

        if (parameters is null)
        {
            return command;
        }

        for (var i = 0; i < parameters.Count; i++)
        {
            command.Parameters.AddWithValue($"@p{i}", ToDbValue(parameters[i]));
        }

        return command;
    }

    private static string RewritePositionalParameters(string sql, int parameterCount)
    {
        if (parameterCount == 0 || !sql.Contains('?'))
        {
            return sql;
        }

        var builder = new global::System.Text.StringBuilder(sql.Length + parameterCount * 2);
        var paramIndex = 0;
        var inSingleQuote = false;
        var inDoubleQuote = false;

        foreach (var ch in sql)
        {
            if (ch == '\'' && !inDoubleQuote)
            {
                inSingleQuote = !inSingleQuote;
                builder.Append(ch);
                continue;
            }

            if (ch == '"' && !inSingleQuote)
            {
                inDoubleQuote = !inDoubleQuote;
                builder.Append(ch);
                continue;
            }

            if (ch == '?' && !inSingleQuote && !inDoubleQuote)
            {
                builder.Append("@p");
                builder.Append(paramIndex++);
                continue;
            }

            builder.Append(ch);
        }

        return builder.ToString();
    }

    private static object ToDbValue(UniDatValue value)
    {
        return value switch
        {
            Universal.UniDatValueScalar scalar => ToDbScalar(scalar.Inner),
            Universal.UniDatValueBinary binary => binary.Inner,
            _ => throw new global::System.NotSupportedException($"Unsupported sqlite parameter type: {value.GetType().Name}"),
        };
    }

    private static object ToDbScalar(UniScalarValue value)
    {
        return value switch
        {
            Universal.UniScalarValueBool v => v.Inner ? 1L : 0L,
            Universal.UniScalarValueU8 v => (long)v.Inner,
            Universal.UniScalarValueI8 v => (long)v.Inner,
            Universal.UniScalarValueU16 v => (long)v.Inner,
            Universal.UniScalarValueI16 v => (long)v.Inner,
            Universal.UniScalarValueU32 v => (long)v.Inner,
            Universal.UniScalarValueI32 v => v.Inner,
            Universal.UniScalarValueU64 v => unchecked((long)v.Inner),
            Universal.UniScalarValueI64 v => v.Inner,
            Universal.UniScalarValueF32 v => (double)v.Inner,
            Universal.UniScalarValueF64 v => v.Inner,
            Universal.UniScalarValueChar v => v.Inner.ToString(),
            Universal.UniScalarValueString v => v.Inner,
            _ => throw new global::System.NotSupportedException($"Unsupported sqlite scalar parameter: {value.GetType().Name}"),
        };
    }

    private static UniTupleRow ReadRow(SqliteDataReader reader)
    {
        var fields = new global::System.Collections.Generic.List<UniDatValue>(reader.FieldCount);
        for (var i = 0; i < reader.FieldCount; i++)
        {
            if (reader.IsDBNull(i))
            {
                throw new global::System.NotSupportedException($"NULL value is not supported for column '{reader.GetName(i)}'");
            }

            fields.Add(ToUniDatValue(reader.GetValue(i), reader.GetDataTypeName(i)));
        }

        return new UniTupleRow
        {
            Fields = fields
        };
    }

    private static UniRecordType BuildTupleDesc(SqliteDataReader reader)
    {
        var fields = new global::System.Collections.Generic.List<UniRecordField>(reader.FieldCount);
        var schema = reader.GetSchemaTable();

        for (var i = 0; i < reader.FieldCount; i++)
        {
            var typeName = schema?.Rows.Count > i
                ? schema.Rows[i]["DataTypeName"]?.ToString()
                : reader.GetDataTypeName(i);
            fields.Add(new UniRecordField
            {
                FieldName = reader.GetName(i),
                FieldType = ToUniDatType(typeName),
            });
        }

        return new UniRecordType
        {
            RecordName = "",
            RecordFields = fields,
        };
    }

    private static UniDatType ToUniDatType(string? sqliteTypeName)
    {
        var normalized = (sqliteTypeName ?? string.Empty).ToUpperInvariant();
        return normalized switch
        {
            "INTEGER" => new Universal.UniDatTypeScalar { Inner = UniScalar.I64 },
            "REAL" => new Universal.UniDatTypeScalar { Inner = UniScalar.F64 },
            "TEXT" => new Universal.UniDatTypeScalar { Inner = UniScalar.String },
            "BLOB" => new Universal.UniDatTypeScalar { Inner = UniScalar.Blob },
            "BOOLEAN" => new Universal.UniDatTypeScalar { Inner = UniScalar.Bool },
            "BOOL" => new Universal.UniDatTypeScalar { Inner = UniScalar.Bool },
            _ => new Universal.UniDatTypeScalar { Inner = UniScalar.String },
        };
    }

    private static UniDatValue ToUniDatValue(object value, string? sqliteTypeName)
    {
        return value switch
        {
            byte[] bytes => new Universal.UniDatValueBinary
            {
                Inner = bytes
            },
            string text => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueString { Inner = text }
            },
            double f64 => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueF64 { Inner = f64 }
            },
            float f32 => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueF32 { Inner = f32 }
            },
            long i64 => new Universal.UniDatValueScalar
            {
                Inner = CreateIntegerScalar(i64, sqliteTypeName)
            },
            int i32 => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueI32 { Inner = i32 }
            },
            short i16 => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueI16 { Inner = i16 }
            },
            byte u8 => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueU8 { Inner = u8 }
            },
            bool b => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueBool { Inner = b }
            },
            _ => new Universal.UniDatValueScalar
            {
                Inner = new Universal.UniScalarValueString
                {
                    Inner = Convert.ToString(value, global::System.Globalization.CultureInfo.InvariantCulture) ?? string.Empty
                }
            },
        };
    }

    private static UniScalarValue CreateIntegerScalar(long value, string? sqliteTypeName)
    {
        var normalized = (sqliteTypeName ?? string.Empty).ToUpperInvariant();
        if (normalized == "BOOLEAN" || normalized == "BOOL")
        {
            return new Universal.UniScalarValueBool { Inner = value != 0 };
        }

        return new Universal.UniScalarValueI64 { Inner = value };
    }

    private static UniError ToUniError(global::System.Exception ex)
    {
        return new UniError
        {
            ErrCode = 1,
            ErrMsg = ex.Message,
        };
    }
}
