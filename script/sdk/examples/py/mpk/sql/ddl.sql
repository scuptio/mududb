CREATE TABLE users (user_id BIGINT PRIMARY KEY, name VARCHAR(100), email VARCHAR(100), created_at BIGINT, updated_at BIGINT);
CREATE TABLE wallets (user_id BIGINT PRIMARY KEY, balance BIGINT, updated_at BIGINT);
