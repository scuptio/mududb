
INSERT INTO users(
    user_id,
    name,
    phone,
    email,
    password,
    created_at
) VALUES (
    1,
    'Alice',
    '12345678',
    'alice@xxx.com',
    'aaa',
    0
);

INSERT INTO users(
    user_id,
    name,
    phone,
    email,
    password,
    created_at
) VALUES (
    2,
    'Bob',
    '22345678',
    'bob@xxx.com',
    'bbb',
    0
);


INSERT INTO wallets (
    user_id,
    balance
) VALUES (
    1,
    10000
);

INSERT INTO wallets (
    user_id,
    balance
) VALUES (
    2,
    10000
);