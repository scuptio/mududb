CREATE TABLE users
(
    user_id    INT,
    name       VARCHAR(100),
    email      VARCHAR(100),
    created_at INT,
    updated_at INT,
    PRIMARY KEY (user_id)
);

CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);
