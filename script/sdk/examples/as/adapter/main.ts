export { cabi_realloc } from "../../../assembly";
import { Database, SqlStmt } from "../../../assembly";
import { create_user, deposit, withdraw, transfer_funds, balance } from "../mpk/assembly/procedures";

// Native execution shares the MPK's business functions and SDK byte-pipe imports.
export function execute(operation: i32, id: i64, to: i64, amount: i64, name: string, email: string): i64 {
  const db = Database.open();
  let result: i64 = 0;
  if (operation == 0) {
    db.command(new SqlStmt("CREATE TABLE users (user_id BIGINT PRIMARY KEY, name VARCHAR(100), email VARCHAR(100), created_at BIGINT, updated_at BIGINT)"));
    db.command(new SqlStmt("CREATE TABLE wallets (user_id BIGINT PRIMARY KEY, balance BIGINT, updated_at BIGINT)"));
  } else if (operation == 1) result = create_user(db.id, id, name, email);
  else if (operation == 2) result = deposit(db.id, id, amount);
  else if (operation == 3) result = withdraw(db.id, id, amount);
  else if (operation == 4) result = transfer_funds(db.id, id, to, amount);
  else if (operation == 5) result = balance(db.id, id);
  db.close();
  return result;
}
