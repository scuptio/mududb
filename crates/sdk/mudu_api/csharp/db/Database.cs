#nullable enable

using mududb.codec;
using mududb.result;
using mududb.sql;
using mududb.sys;

namespace mududb.db;

/// <summary>
/// Canonical facade `mududb.db`: the database session handle, mirroring the
/// AssemblyScript `Database` semantics (`doc/dev/binding_api_surface.md`,
/// section "Canonical facade surface"). Obtain one with
/// <see cref="Open(string)"/> and release it with <see cref="Close"/>.
/// Errors carried by a syscall's `UniError` arm surface as thrown
/// <see cref="global::System.InvalidOperationException"/>.
/// </summary>
public sealed class Database
{
    private Database(UniOid id)
    {
        Id = id;
    }

    /// <summary>
    /// The session object id of this handle.
    /// </summary>
    public UniOid Id { get; }

    /// <summary>
    /// Open a session. An empty `uri` selects the default worker (worker oid
    /// 0); otherwise `uri` must be a decimal u128 worker object id, split
    /// into `UniOid { H = high 64 bits, L = low 64 bits }` — the same rule as
    /// the AssemblyScript `parseWorkerOid`. Invalid text throws
    /// <see cref="global::System.ArgumentException"/>.
    /// </summary>
    public static Database Open(string uri = "")
    {
        var workerId = ParseWorkerOid(uri);
        var request = UniSyscall.EncodeOpenSessionRequest(workerId);
        var response = MuduSysCallApi.OpenRaw(request);
        var result = UniSyscall.DecodeOpenSessionResult(response);
        if (result.IsErr)
        {
            throw new global::System.InvalidOperationException(result.Error.GetValueOrDefault().ErrMsg);
        }

        return new Database(result.Value);
    }

    /// <summary>
    /// Close the session.
    /// </summary>
    public void Close()
    {
        var request = UniSyscall.EncodeCloseSessionRequest(Id);
        var response = MuduSysCallApi.CloseRaw(request);
        var error = UniSyscall.DecodeCloseSessionResult(response);
        if (error is not null)
        {
            throw new global::System.InvalidOperationException(error.GetValueOrDefault().ErrMsg);
        }
    }

    /// <summary>
    /// Run a `SELECT` statement and return the full result set (the host
    /// drains all rows into the first response; iteration never goes back to
    /// the wire).
    /// </summary>
    public ResultSet Query(SqlStmt stmt, SqlParams? values = null)
    {
        var argv = new UniQueryArgv
        {
            Oid = Id,
            Query = new UniSqlStmt { SqlString = stmt.Sql },
            ParamList = ToUniSqlParam(values),
            // `param-desc` stays null — the same convention as the AS guest.
        };
        var request = MuduSysCallApi.SerializeQuery(argv);
        var response = MuduSysCallApi.QueryRaw(request);
        var result = MuduSysCallApi.DeserializeQueryResult(response);
        return result.Kind() switch
        {
            UniQueryReturnKind.Ok => new ResultSet(UniQueryReturnOk.AsOk(result).Inner),
            UniQueryReturnKind.Err => throw new global::System.InvalidOperationException(UniQueryReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown query result kind"),
        };
    }

    /// <summary>
    /// Run an `INSERT` / `UPDATE` / `DELETE` statement and return the number
    /// of affected rows.
    /// </summary>
    public ulong Command(SqlStmt stmt, SqlParams? values = null)
    {
        var argv = new UniCommandArgv
        {
            Oid = Id,
            Command = new UniSqlStmt { SqlString = stmt.Sql },
            ParamList = ToUniSqlParam(values),
        };
        var request = MuduSysCallApi.SerializeCommand(argv);
        var response = MuduSysCallApi.CommandRaw(request);
        var result = MuduSysCallApi.DeserializeCommandResult(response);
        return AffectedRows(result);
    }

    /// <summary>
    /// Run a statement through the batch syscall path; same argument and
    /// return shape as <see cref="Command"/>.
    /// </summary>
    public ulong Batch(SqlStmt stmt, SqlParams? values = null)
    {
        var argv = new UniCommandArgv
        {
            Oid = Id,
            Command = new UniSqlStmt { SqlString = stmt.Sql },
            ParamList = ToUniSqlParam(values),
        };
        var request = MuduSysCallApi.SerializeBatch(argv);
        var response = MuduSysCallApi.BatchRaw(request);
        var result = MuduSysCallApi.DeserializeBatchResult(response);
        return AffectedRows(result);
    }

    // The open "uri" is either empty (the default worker) or a decimal u128
    // worker object id; mirrors the AssemblyScript `parseWorkerOid`.
    private static UniOid ParseWorkerOid(string uri)
    {
        if (uri.Length == 0)
        {
            return new UniOid { H = 0, L = 0 };
        }

        global::System.UInt128 value = 0;
        foreach (var ch in uri)
        {
            if (ch < '0' || ch > '9')
            {
                throw new global::System.ArgumentException(
                    $"open uri must be empty or a numeric worker object id: {uri}", nameof(uri));
            }

            try
            {
                value = checked(value * 10 + (uint)(ch - '0'));
            }
            catch (global::System.OverflowException)
            {
                throw new global::System.ArgumentException(
                    $"worker object id does not fit into u128: {uri}", nameof(uri));
            }
        }

        return new UniOid { H = (ulong)(value >> 64), L = (ulong)value };
    }

    private static UniSqlParam ToUniSqlParam(SqlParams? values)
    {
        return values?.ToUniSqlParam() ?? new UniSqlParam { Params = [] };
    }

    private static ulong AffectedRows(UniCommandReturn result)
    {
        return result.Kind() switch
        {
            UniCommandReturnKind.Ok => UniCommandReturnOk.AsOk(result).Inner.AffectedRows,
            UniCommandReturnKind.Err => throw new global::System.InvalidOperationException(UniCommandReturnErr.AsErr(result).Inner.ErrMsg),
            _ => throw new global::System.InvalidOperationException("Unknown command result kind"),
        };
    }
}
