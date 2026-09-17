package main

// Negative fixture: arity mismatch and unknown table.
func getBalance(session muduOid, userID int64) (int64, error) {
	rows, err := sysQuery(session, "SELECT balance FROM wallets WHERE user_id = ?", userID, int64(0))
	_, _ = rows, err
	return 0, nil
}

func deleteWallet(session muduOid, userID int64) (int64, error) {
	return sysCommand(session, "DELETE FROM wallet WHERE user_id = ?", userID)
}
