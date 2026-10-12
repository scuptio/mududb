package main

// Wallet business functions shared by MPK and native adapter entry points.
func queryBalance(session muduOid, userId int64) (int64, error) {
	rows, err := sysQuery(session, "SELECT balance FROM wallets WHERE user_id = ?", userId)
	if err != nil {
		return 0, err
	}
	if len(rows) == 0 || len(rows[0]) == 0 {
		return 0, &domainError{errCodeEntityNotFound, "wallet not found"}
	}
	balance, ok := rows[0][0].(int64)
	if !ok {
		return 0, &domainError{errCodeInternal, "wallet balance is not an i64"}
	}
	return balance, nil
}

func setBalance(session muduOid, userId int64, balance int64) (int64, error) {
	updated, err := sysCommand(session,
		"UPDATE wallets SET balance = ? WHERE user_id = ?", balance, userId)
	if err != nil {
		return 0, err
	}
	if updated != 1 {
		return 0, &domainError{errCodeDomainViolation, "wallet update failed"}
	}
	return balance, nil
}

// mudu-proc
func createUser(session muduOid, userId int64, name string, email string) (int64, error) {
	users, err := sysCommand(session,
		"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
		userId, name, email, int64(0), int64(0))
	if err != nil {
		return 0, err
	}
	if users != 1 {
		return 0, &domainError{errCodeDomainViolation, "create user failed"}
	}

	wallets, err := sysCommand(session,
		"INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
		userId, int64(0), int64(0))
	if err != nil {
		return 0, err
	}
	if wallets != 1 {
		return 0, &domainError{errCodeDomainViolation, "create wallet failed"}
	}
	return userId, nil
}

// mudu-proc
func deposit(session muduOid, userId int64, amount int64) (int64, error) {
	if amount <= 0 {
		return 0, &domainError{errCodeDomainViolation, "amount must be positive"}
	}
	balance, err := queryBalance(session, userId)
	if err != nil {
		return 0, err
	}
	return setBalance(session, userId, balance+amount)
}

// mudu-proc
func withdraw(session muduOid, userId int64, amount int64) (int64, error) {
	if amount <= 0 {
		return 0, &domainError{errCodeDomainViolation, "amount must be positive"}
	}
	current, err := queryBalance(session, userId)
	if err != nil {
		return 0, err
	}
	if current < amount {
		return 0, &domainError{errCodeDomainViolation, "insufficient funds"}
	}
	return setBalance(session, userId, current-amount)
}

// mudu-proc
func transferFunds(session muduOid, fromUserId int64, toUserId int64, amount int64) (int64, error) {
	if amount <= 0 {
		return 0, &domainError{errCodeDomainViolation, "amount must be positive"}
	}
	if fromUserId == toUserId {
		return 0, &domainError{errCodeDomainViolation, "cannot transfer to self"}
	}
	currentFrom, err := queryBalance(session, fromUserId)
	if err != nil {
		return 0, err
	}
	currentTo, err := queryBalance(session, toUserId)
	if err != nil {
		return 0, err
	}
	if currentFrom < amount {
		return 0, &domainError{errCodeDomainViolation, "insufficient funds"}
	}

	newFrom, err := setBalance(session, fromUserId, currentFrom-amount)
	if err != nil {
		return 0, err
	}
	if _, err := setBalance(session, toUserId, currentTo+amount); err != nil {
		return 0, err
	}
	return newFrom, nil
}

// mudu-proc
func balance(session muduOid, userId int64) (int64, error) {
	return queryBalance(session, userId)
}
