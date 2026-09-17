// Negative fixture: arity mismatch and unknown table.
internal static class Procedures
{
    public static long GetBalance(MuduOid session, long userId)
    {
        var rows = MuduSys.Query(session, "SELECT balance FROM wallets WHERE user_id = ?", userId, 0L);
        return rows;
    }

    public static long DeleteWallet(MuduOid session, long userId)
    {
        return MuduSys.Command(session, "DELETE FROM wallet WHERE user_id = ?", userId);
    }
}
