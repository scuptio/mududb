package main

import (
	"sort"

	gentypes "wallet_go/gentypes"
)

// wallet-go business logic: mirrors wallet-py/procedures.py
// procedure-for-procedure (create_user, deposit, withdraw, transfer_funds,
// balance), written against the syscall layer in mudusys.go, which frames
// SyscallPayload v1 (MSSP) messages over the `mududb:api/system` byte pipe
// with the in-repo Go binding (`types` + `codec`). update_profile
// demonstrates a user-defined record type (wit/types.wit, generated into
// wallet_go/gentypes by mgen) as a procedure parameter and return type.
//
// Convention (same as every guest): the first parameter of each
// `// mudu-proc` function is the bound session OID, injected by the
// mtp-generated adapter from `UniProcedureParam.session`; the remaining
// parameters arrive positionally in `param_list`.

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

// updateProfile stores a user-defined record argument and returns the stored
// record. The `profile` record type is declared in wit/types.wit and
// generated into wallet_go/gentypes by mgen; the mtp adapter decodes the
// positional record envelope through the binding bridge
// (types.RecordFieldValues) composed with gentypes.ProfileFromValue, and
// encodes the returned record symmetrically.
//
// Storage design: the record flattens into the `profile` table columns
// (bool/u32 as INT — the host carries both as i32); the nested optional
// address flattens into the nullable home_city/home_zip columns (both NULL
// when the option is absent); the tag list lands in the `profile_tags`
// side table keyed by (user_id, idx) — the engine requires a complete key
// for DELETE and has no ORDER BY, so queryProfile re-sorts by idx in Go.
//
// mudu-proc
func updateProfile(session muduOid, userId int64, profile gentypes.Profile) (gentypes.Profile, error) {
	vip := int64(0)
	if profile.Vip {
		vip = 1
	}
	var homeCity, homeZip any
	if profile.Home != nil {
		homeCity = profile.Home.City
		homeZip = profile.Home.Zip
	}
	updated, err := sysCommand(session,
		"UPDATE profile SET display_name = ?, level = ?, vip = ?, home_city = ?, home_zip = ? WHERE user_id = ?",
		profile.DisplayName, int64(profile.Level), vip, homeCity, homeZip, userId)
	if err != nil {
		return gentypes.Profile{}, err
	}
	if updated == 0 {
		inserted, err := sysCommand(session,
			"INSERT INTO profile (user_id, display_name, level, vip, home_city, home_zip) VALUES (?, ?, ?, ?, ?, ?)",
			userId, profile.DisplayName, int64(profile.Level), vip, homeCity, homeZip)
		if err != nil {
			return gentypes.Profile{}, err
		}
		if inserted != 1 {
			return gentypes.Profile{}, &domainError{errCodeDomainViolation, "insert profile failed"}
		}
	}
	// The engine requires a complete primary key for DELETE, so the old
	// tags are first read by the key-prefix predicate and then deleted one
	// by one.
	oldTags, err := sysQuery(session,
		"SELECT idx, tag FROM profile_tags WHERE user_id = ?", userId)
	if err != nil {
		return gentypes.Profile{}, err
	}
	for _, row := range oldTags {
		if len(row) == 0 {
			return gentypes.Profile{}, &domainError{errCodeInternal, "profile tag row shape mismatch"}
		}
		idx, ok := row[0].(int64)
		if !ok {
			return gentypes.Profile{}, &domainError{errCodeInternal, "profile tag idx is not an i64"}
		}
		deleted, err := sysCommand(session,
			"DELETE FROM profile_tags WHERE user_id = ? AND idx = ?", userId, idx)
		if err != nil {
			return gentypes.Profile{}, err
		}
		if deleted != 1 {
			return gentypes.Profile{}, &domainError{errCodeDomainViolation, "delete profile tag failed"}
		}
	}
	for i, tag := range profile.Tags {
		inserted, err := sysCommand(session,
			"INSERT INTO profile_tags (user_id, idx, tag) VALUES (?, ?, ?)",
			userId, int64(i), tag)
		if err != nil {
			return gentypes.Profile{}, err
		}
		if inserted != 1 {
			return gentypes.Profile{}, &domainError{errCodeDomainViolation, "insert profile tag failed"}
		}
	}
	return queryProfile(session, userId)
}

// queryProfile rebuilds the stored profile record: flat columns, the
// nullable address columns, and the side-table tags re-sorted by idx.
func queryProfile(session muduOid, userId int64) (gentypes.Profile, error) {
	rows, err := sysQuery(session,
		"SELECT display_name, level, vip, home_city, home_zip FROM profile WHERE user_id = ?", userId)
	if err != nil {
		return gentypes.Profile{}, err
	}
	if len(rows) == 0 {
		return gentypes.Profile{}, &domainError{errCodeEntityNotFound, "profile not found"}
	}
	row := rows[0]
	if len(row) != 5 {
		return gentypes.Profile{}, &domainError{errCodeInternal, "profile row shape mismatch"}
	}
	displayName, ok := row[0].(string)
	if !ok {
		return gentypes.Profile{}, &domainError{errCodeInternal, "profile display_name is not a string"}
	}
	level, ok := row[1].(int64)
	if !ok {
		return gentypes.Profile{}, &domainError{errCodeInternal, "profile level is not an i64"}
	}
	vip, ok := row[2].(int64)
	if !ok {
		return gentypes.Profile{}, &domainError{errCodeInternal, "profile vip is not an i64"}
	}
	var home *gentypes.Address
	if row[3] != nil && row[4] != nil {
		city, cityOk := row[3].(string)
		zip, zipOk := row[4].(string)
		if !cityOk || !zipOk {
			return gentypes.Profile{}, &domainError{errCodeInternal, "profile address columns are not strings"}
		}
		home = &gentypes.Address{City: city, Zip: zip}
	}

	tagRows, err := sysQuery(session,
		"SELECT idx, tag FROM profile_tags WHERE user_id = ?", userId)
	if err != nil {
		return gentypes.Profile{}, err
	}
	sort.Slice(tagRows, func(i, j int) bool {
		left, leftOk := tagRows[i][0].(int64)
		right, rightOk := tagRows[j][0].(int64)
		return leftOk && rightOk && left < right
	})
	tags := make([]string, 0, len(tagRows))
	for _, tagRow := range tagRows {
		if len(tagRow) != 2 {
			return gentypes.Profile{}, &domainError{errCodeInternal, "profile tag row shape mismatch"}
		}
		tag, ok := tagRow[1].(string)
		if !ok {
			return gentypes.Profile{}, &domainError{errCodeInternal, "profile tag is not a string"}
		}
		tags = append(tags, tag)
	}

	return gentypes.Profile{
		DisplayName: displayName,
		Level:       uint32(level),
		Vip:         vip != 0,
		Tags:        tags,
		Home:        home,
	}, nil
}
