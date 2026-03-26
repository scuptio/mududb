using Mudu.Api;

var dbPath = Path.Combine(AppContext.BaseDirectory, "demo.db");
Environment.SetEnvironmentVariable("MUDU_MOCK_SQLITE_PATH", dbPath);

RunCommand(
    "create table if not exists demo_users (" +
    "id integer primary key autoincrement, " +
    "name text not null, " +
    "score integer not null" +
    ")"
);

RunCommand("delete from demo_users");

InsertUser("alice", 10);
InsertUser("bob", 20);

var queryResult = global::Mudu.Api.Mudu.Query(new UniQueryArgv
{
    Oid = new UniOid { H = 0, L = 0 },
    Query = new UniSqlStmt
    {
        SqlString = "select id, name, score from demo_users where score >= ? order by id"
    },
    ParamList = new UniSqlParam
    {
        Params = new()
        {
            new Universal.UniDatValuePrimitive
            {
                Inner = new Universal.UniPrimitiveValueI32
                {
                    Inner = 10
                }
            }
        }
    }
});

if (queryResult.IsErr)
{
    var error = queryResult.Error!.Value;
    Console.WriteLine($"query failed: {error.ErrCode} {error.ErrMsg}");
    return;
}

var ok = queryResult.RequireOk();
Console.WriteLine($"db: {dbPath}");
Console.WriteLine($"rows: {ok.ResultSet.RowSet.Count}");

foreach (var row in ok.ResultSet.RowSet)
{
    var id = ((Universal.UniPrimitiveValueI64)((Universal.UniDatValuePrimitive)row.Fields[0]).Inner).Inner;
    var name = ((Universal.UniPrimitiveValueString)((Universal.UniDatValuePrimitive)row.Fields[1]).Inner).Inner;
    var score = ((Universal.UniPrimitiveValueI64)((Universal.UniDatValuePrimitive)row.Fields[2]).Inner).Inner;
    Console.WriteLine($"{id}: {name} -> {score}");
}

static void InsertUser(string name, int score)
{
    var response = global::Mudu.Api.Mudu.Command(new UniCommandArgv
    {
        Oid = new UniOid { H = 0, L = 0 },
        Command = new UniSqlStmt
        {
            SqlString = "insert into demo_users(name, score) values(?, ?)"
        },
        ParamList = new UniSqlParam
        {
            Params = new()
            {
                new Universal.UniDatValuePrimitive
                {
                    Inner = new Universal.UniPrimitiveValueString
                    {
                        Inner = name
                    }
                },
                new Universal.UniDatValuePrimitive
                {
                    Inner = new Universal.UniPrimitiveValueI32
                    {
                        Inner = score
                    }
                }
            }
        }
    });

    if (response.IsErr)
    {
        var error = response.Error!.Value;
        throw new InvalidOperationException($"insert failed: {error.ErrCode} {error.ErrMsg}");
    }
}

static void RunCommand(string sql)
{
    var response = global::Mudu.Api.Mudu.Command(new UniCommandArgv
    {
        Oid = new UniOid { H = 0, L = 0 },
        Command = new UniSqlStmt
        {
            SqlString = sql
        },
        ParamList = new UniSqlParam
        {
            Params = new()
        }
    });

    if (response.IsErr)
    {
        var error = response.Error!.Value;
        throw new InvalidOperationException($"command failed: {error.ErrCode} {error.ErrMsg}");
    }
}
