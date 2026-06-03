import {
  Oid,
  ResultSet,
  SqlStmt,
  Value,
  ValueList,
} from "./mududb_binding";
import { witCommand, witQuery } from "./mududb_wit";

function command(id: Oid, sql: string, values: ValueList = new ValueList()): i64 {
  return <i64>witCommand(id, new SqlStmt(sql).raw, values.raw).unwrap();
}

function queryBalance(id: Oid, userId: i64): i64 {
  const params = new ValueList();
  params.bind(0, Value.int64(userId));

  const rows = new ResultSet(witQuery(
    id,
    new SqlStmt("SELECT balance FROM wallets WHERE user_id = ?").raw,
    params.raw,
  ).unwrap());

  if (!rows.next()) {
    throw new Error("wallet not found");
  }
  return rows.currentRow().valueByName("balance").asInt64();
}

function setBalance(id: Oid, userId: i64, balance: i64): i64 {
  const params = new ValueList();
  params.bind(0, Value.int64(balance));
  params.bind(1, Value.int64(userId));

  const updated = command(
    id,
    "UPDATE wallets SET balance = ? WHERE user_id = ?",
    params,
  );
  if (updated != 1) {
    throw new Error("wallet update failed");
  }
  return balance;
}

/**mudu-proc*/
export function create_user(id: Oid, user_id: i64, name: string, email: string): i64 {
  const userParams = new ValueList();
  userParams.bind(0, Value.int64(user_id));
  userParams.bind(1, Value.text(name));
  userParams.bind(2, Value.text(email));
  userParams.bind(3, Value.int64(0));
  userParams.bind(4, Value.int64(0));
  const users = command(
    id,
    "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    userParams,
  );
  if (users != 1) {
    throw new Error("create user failed");
  }

  const walletParams = new ValueList();
  walletParams.bind(0, Value.int64(user_id));
  walletParams.bind(1, Value.int64(0));
  walletParams.bind(2, Value.int64(0));
  const wallets = command(
    id,
    "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
    walletParams,
  );
  if (wallets != 1) {
    throw new Error("create wallet failed");
  }
  return user_id;
}

/**mudu-proc*/
export function deposit(id: Oid, user_id: i64, amount: i64): i64 {
  if (amount <= 0) {
    throw new Error("amount must be positive");
  }

  const balance = queryBalance(id, user_id);
  return setBalance(id, user_id, balance + amount);
}

/**mudu-proc*/
export function withdraw(id: Oid, user_id: i64, amount: i64): i64 {
  if (amount <= 0) {
    throw new Error("amount must be positive");
  }

  const current = queryBalance(id, user_id);
  if (current < amount) {
    throw new Error("insufficient funds");
  }
  return setBalance(id, user_id, current - amount);
}

/**mudu-proc*/
export function transfer_funds(
  id: Oid,
  from_user_id: i64,
  to_user_id: i64,
  amount: i64,
): i64 {
  if (amount <= 0) {
    throw new Error("amount must be positive");
  }
  if (from_user_id == to_user_id) {
    throw new Error("cannot transfer to self");
  }

  const currentFrom = queryBalance(id, from_user_id);
  const currentTo = queryBalance(id, to_user_id);
  if (currentFrom < amount) {
    throw new Error("insufficient funds");
  }

  const newFrom = setBalance(id, from_user_id, currentFrom - amount);
  setBalance(id, to_user_id, currentTo + amount);
  return newFrom;
}

/**mudu-proc*/
export function balance(id: Oid, user_id: i64): i64 {
  return queryBalance(id, user_id);
}
