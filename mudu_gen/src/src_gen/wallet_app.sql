CREATE TABLE users
(
    user_id    INT,
    name       VARCHAR(100),
    phone      VARCHAR(20) ,
    email      VARCHAR(100),
    password   VARCHAR(255),
    created_at INT,
    updated_at INT,
    PRIMARY KEY (user_id)
);


CREATE TABLE wallets
(
    user_id INT PRIMARY KEY,
    balance INT,
    updated_at INT
);


CREATE TABLE transactions
(
    trans_id   CHAR(256),
    trans_type CHAR(256),
    from_user  INT,
    to_user    INT,
    amount     INT,
    created_at INT,
    PRIMARY KEY (trans_id)
);

CREATE TABLE orders
(
    order_id   INT,
    user_id    INT,
    merch_id   INT,
    amount     INT,
    created_at INT,
    PRIMARY KEY (order_id)
);
