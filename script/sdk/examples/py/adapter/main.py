"""Run Wallet business functions locally through the real native adapter."""
import argparse
import json
from pathlib import Path
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[2]))  # SDK root, also works without pip install
sys.path.insert(0, str(HERE.parent / 'mpk'))
from mududb.adapter import use_adapter
from mududb.db import Database
from mududb.sql import SqlStmt
import procedures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['init', 'create', 'deposit', 'withdraw', 'transfer', 'balance', 'scenario'])
    parser.add_argument('values', nargs='*')
    args = parser.parse_args()
    use_adapter()
    db = Database.open()
    try:
        if args.operation == 'init':
            for sql in (HERE.parent/'mpk/sql/ddl.sql').read_text().split(';'):
                if sql.strip(): db.command(SqlStmt(sql))
            return
        ops = {'create': procedures.create_user, 'deposit': procedures.deposit,
               'withdraw': procedures.withdraw, 'transfer': procedures.transfer_funds, 'balance': procedures.balance}
        if args.operation == 'scenario':
            # Fresh database required: duplicate keys fail instead of deleting data.
            procedures.create_user(db.id, 1001, 'Alice', 'alice@example.com')
            procedures.create_user(db.id, 1002, 'Bob', 'bob@example.com')
            procedures.deposit(db.id, 1001, 1000)
            procedures.withdraw(db.id, 1001, 100)
            procedures.transfer_funds(db.id, 1001, 1002, 250)
            before = (procedures.balance(db.id, 1001), procedures.balance(db.id, 1002))
            assert before == (650, 250), before
            try:
                procedures.withdraw(db.id, 1001, 10000)
            except Exception as error:
                if str(error) != 'insufficient funds': raise
            else:
                raise AssertionError('insufficient funds was accepted')
            assert procedures.balance(db.id, 1001) == 650
            print(json.dumps({'alice': 650, 'bob': 250}))
            return
        values = [int(args.values[0]), *args.values[1:]] if args.operation == 'create' else list(map(int, args.values))
        print(ops[args.operation](db.id, *values))
    finally:
        db.close()

if __name__ == '__main__':
    main()
