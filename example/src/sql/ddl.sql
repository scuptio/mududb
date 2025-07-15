
DROP TABLE IF EXISTS users CASCADE;
DROP TABLE IF EXISTS wallets CASCADE;
DROP TABLE IF EXISTS transactions CASCADE;
DROP TABLE IF EXISTS orders CASCADE;

CREATE TABLE users (
    user_id INT,
    name VARCHAR(100) NOT NULL,
    phone VARCHAR(20) NOT NULL ,
    email VARCHAR(100) ,
    password VARCHAR(255) NOT NULL,
    created_at INT,
    PRIMARY KEY(user_id)
);


CREATE TABLE wallets (
    user_id INT PRIMARY KEY,
    balance INT NOT NULL,
);


CREATE TABLE transactions (
    trans_id CHAR(256),
    from_user INT,
    to_user INT,
    amount INT NOT NULL,
    created_at INT,
    PRIMARY KEY(trans_id)
);

CREATE TABLE orders (
    order_id INT,
    user_id INT NOT NULL,
    merch_id INT NOT NULL,
    amount INT NOT NULL,
    created_at INT,
    PRIMARY KEY (order_id)
);
