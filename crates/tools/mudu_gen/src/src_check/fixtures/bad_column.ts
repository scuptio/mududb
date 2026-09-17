// Negative fixture: `balances` is not a column of wallets.
function queryBalance(id: i64, userId: i64): i64 {
  const rows = new SqlStmt("SELECT balances FROM wallets WHERE user_id = ?").raw;
  return 0;
}
