using mududb.db;
using mududb.sql;
using mududb.sys;
using mududb.types;
using WalletCs;

NativeAdapter.Use();
var db = Database.Open();
try
{
    var session = new MuduOid(db);
    if (args.Length == 0) throw new ArgumentException("Usage: init | scenario | balance ID | create ID NAME EMAIL | deposit ID AMOUNT | withdraw ID AMOUNT | transfer FROM TO AMOUNT");
    switch (args[0])
    {
        case "init":
            foreach (var sql in File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "ddl.sql")).Split(';'))
                if (!string.IsNullOrWhiteSpace(sql)) db.Command(new SqlStmt(sql));
            break;
        case "scenario":
            Procedures.CreateUser(session, 1001, "Alice", "alice@example.com");
            Procedures.CreateUser(session, 1002, "Bob", "bob@example.com");
            Procedures.Deposit(session, 1001, 1000);
            Procedures.Withdraw(session, 1001, 100);
            Procedures.TransferFunds(session, 1001, 1002, 250);
            if (Procedures.Balance(session, 1001) != 650 || Procedures.Balance(session, 1002) != 250) throw new Exception("Balance mismatch");
            try { Procedures.Withdraw(session, 1001, 10000); throw new Exception("Insufficient funds accepted"); }
            catch (WalletCsException e) when (e.Message == "insufficient funds") { }
            Console.WriteLine("{\"alice\":650,\"bob\":250}");
            break;
        case "create": Console.WriteLine(Procedures.CreateUser(session, long.Parse(args[1]), args[2], args[3])); break;
        case "deposit": Console.WriteLine(Procedures.Deposit(session, long.Parse(args[1]), long.Parse(args[2]))); break;
        case "withdraw": Console.WriteLine(Procedures.Withdraw(session, long.Parse(args[1]), long.Parse(args[2]))); break;
        case "transfer": Console.WriteLine(Procedures.TransferFunds(session, long.Parse(args[1]), long.Parse(args[2]), long.Parse(args[3]))); break;
        case "balance": Console.WriteLine(Procedures.Balance(session, long.Parse(args[1]))); break;
        default: throw new ArgumentException("Unknown operation");
    }
}
finally { db.Close(); }

namespace WalletCs
{
    internal readonly record struct MuduOid(Database Database);
    internal static class MuduSys
    {
        private static SqlParams Params(object?[] values)
        {
            var p = new SqlParams();
            for (int i=0; i<values.Length; i++)
                switch (values[i]) { case long n: p.Bind(i,n); break; case string s: p.Bind(i,s); break; default: throw new ArgumentException("Unsupported wallet parameter"); }
            return p;
        }
        public static ulong Command(MuduOid session, string sql, params object?[] args) => session.Database.Command(new SqlStmt(sql), Params(args));
        public static List<object?[]> Query(MuduOid session, string sql, params object?[] args)
        {
            var result = session.Database.Query(new SqlStmt(sql), Params(args));
            var rows = new List<object?[]>();
            while (result.Next())
            {
                var scalar = UniDataValueScalar.AsScalar(result.CurrentRow().Value(0)).Inner;
                long value = scalar switch { UniScalarValueI64 n => n.Inner, UniScalarValueI32 n => n.Inner, _ => throw new Exception("Expected integer balance") };
                rows.Add([value]);
            }
            return rows;
        }
    }
}
