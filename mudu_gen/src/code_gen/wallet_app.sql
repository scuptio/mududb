
CREATE TABLE users (
    user_id INT,
    name VARCHAR(100) NOT NULL,
    phone VARCHAR(20) NOT NULL ,
    email VARCHAR(100) ,
    password VARCHAR(255) NOT NULL,
    created_at TIMESTAMP,
    PRIMARY KEY(user_id)
);


CREATE TABLE wallets (
    user_id INT PRIMARY KEY,
    balance INT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);


CREATE TABLE transactions (
    trans_id INT,
    from_user INT,
    to_user INT,
    amount DECIMAL(15, 2) NOT NULL,
    created_at TIMESTAMP,
    PRIMARY KEY(trans_id)
);

CREATE TABLE orders (
    order_id INT AUTO_INCREMENT PRIMARY KEY,
    user_id INT NOT NULL,
    merch_id INT NOT NULL,
    amount DECIMAL(15, 2) NOT NULL,
    created_at TIMESTAMP,
    PRIMARY KEY (order_id)
);
