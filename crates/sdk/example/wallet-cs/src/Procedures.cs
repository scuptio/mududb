#nullable enable

namespace WalletCs;

/// <summary>
/// Wallet business logic, mirroring the AssemblyScript wallet-as procedures
/// (<c>crates/sdk/example/wallet-as/assembly/procedures.ts</c>) one to one:
/// every procedure takes the session OID from the procedure parameter and
/// returns an <c>i64</c> (the affected user id or the resulting balance).
/// SQL goes through <see cref="MuduSys"/>, which frames SyscallPayload v1
/// (MSSP) messages over the <c>mududb:api/system</c> byte pipe.
/// </summary>
internal static class Procedures
{
    // mudu-proc
    public static long CreateUser(MuduOid session, long userId, string name, string email)
    {
        var users = MuduSys.Command(session,
            "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            userId, name, email, 0L, 0L);
        if (users != 1)
        {
            throw new WalletCsException(ErrorCodes.DomainViolation, "create user failed");
        }

        var wallets = MuduSys.Command(session,
            "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
            userId, 0L, 0L);
        if (wallets != 1)
        {
            throw new WalletCsException(ErrorCodes.DomainViolation, "create wallet failed");
        }

        return userId;
    }

    // mudu-proc
    public static long Deposit(MuduOid session, long userId, long amount)
    {
        if (amount <= 0)
        {
            throw new WalletCsException(ErrorCodes.InvalidArgument, "amount must be positive");
        }

        var balance = QueryBalance(session, userId);
        return SetBalance(session, userId, balance + amount);
    }

    // mudu-proc
    public static long Withdraw(MuduOid session, long userId, long amount)
    {
        if (amount <= 0)
        {
            throw new WalletCsException(ErrorCodes.InvalidArgument, "amount must be positive");
        }

        var balance = QueryBalance(session, userId);
        if (balance < amount)
        {
            throw new WalletCsException(ErrorCodes.DomainViolation, "insufficient funds");
        }

        return SetBalance(session, userId, balance - amount);
    }

    // mudu-proc
    public static long TransferFunds(MuduOid session, long fromUserId, long toUserId, long amount)
    {
        if (amount <= 0)
        {
            throw new WalletCsException(ErrorCodes.InvalidArgument, "amount must be positive");
        }

        if (fromUserId == toUserId)
        {
            throw new WalletCsException(ErrorCodes.InvalidArgument, "cannot transfer to self");
        }

        var fromBalance = QueryBalance(session, fromUserId);
        var toBalance = QueryBalance(session, toUserId);
        if (fromBalance < amount)
        {
            throw new WalletCsException(ErrorCodes.DomainViolation, "insufficient funds");
        }

        var newFromBalance = SetBalance(session, fromUserId, fromBalance - amount);
        SetBalance(session, toUserId, toBalance + amount);
        return newFromBalance;
    }

    // mudu-proc
    public static long Balance(MuduOid session, long userId)
    {
        return QueryBalance(session, userId);
    }

    private static long QueryBalance(MuduOid session, long userId)
    {
        var rows = MuduSys.Query(session, "SELECT balance FROM wallets WHERE user_id = ?", userId);
        if (rows.Count == 0 || rows[0][0] is not long balance)
        {
            throw new WalletCsException(ErrorCodes.EntityNotFound, "wallet not found");
        }

        return balance;
    }

    private static long SetBalance(MuduOid session, long userId, long balance)
    {
        var updated = MuduSys.Command(session,
            "UPDATE wallets SET balance = ? WHERE user_id = ?",
            balance, userId);
        if (updated != 1)
        {
            throw new WalletCsException(ErrorCodes.DomainViolation, "wallet update failed");
        }

        return balance;
    }
}

/// <summary>
/// Numeric mirrors of the host's <c>mudu::error::ErrorCode</c> discriminants
/// carried in the <c>UniError</c> result arm.
/// </summary>
internal static class ErrorCodes
{
    public const uint EntityNotFound = 50009;
    public const uint DomainViolation = 50017;
    public const uint InvalidArgument = 50029;
    public const uint Internal = 50000;
}

/// <summary>
/// A domain error raised by a wallet procedure; the byte-pipe dispatcher
/// encodes it as the <c>{1: UniError}</c> arm of the procedure result.
/// </summary>
internal sealed class WalletCsException(uint code, string message)
    : global::System.Exception(message)
{
    public uint Code { get; } = code;
}
