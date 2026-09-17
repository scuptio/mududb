CREATE TABLE users
(
    user_id    INT,
    name       VARCHAR(100),
    phone      VARCHAR(20),
    email      VARCHAR(100),
    password   VARCHAR(255),
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

-- wallet-go update_profile: the profile record flattened into columns
-- (bool/u32 ride as INT; the nested optional address is the nullable
-- home_city/home_zip pair), tags in a side table.
CREATE TABLE profile
(
    user_id      INT PRIMARY KEY,
    display_name VARCHAR(100),
    level        INT,
    vip          INT,
    home_city    VARCHAR(100),
    home_zip     VARCHAR(20)
);

CREATE TABLE profile_tags
(
    user_id INT,
    idx     INT,
    tag     VARCHAR(100),
    PRIMARY KEY (user_id, idx)
);
