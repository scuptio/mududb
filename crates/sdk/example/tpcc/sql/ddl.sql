CREATE TABLE IF NOT EXISTS warehouse (
    w_id INTEGER PRIMARY KEY,
    w_name TEXT NOT NULL,
    w_tax INTEGER NOT NULL,
    w_ytd NUMERIC(12,2) NOT NULL
);

CREATE TABLE IF NOT EXISTS district (
    d_id INTEGER NOT NULL,
    d_w_id INTEGER NOT NULL,
    d_name TEXT NOT NULL,
    d_tax INTEGER NOT NULL,
    d_ytd NUMERIC(12,2) NOT NULL,
    d_next_o_id INTEGER NOT NULL,
    d_last_delivery_o_id INTEGER NOT NULL,
    PRIMARY KEY (d_w_id, d_id)
);

CREATE TABLE IF NOT EXISTS customer (
    c_id INTEGER NOT NULL,
    c_d_id INTEGER NOT NULL,
    c_w_id INTEGER NOT NULL,
    c_first TEXT NOT NULL,
    c_last TEXT NOT NULL,
    c_discount INTEGER NOT NULL,
    c_credit TEXT NOT NULL,
    c_balance NUMERIC(12,2) NOT NULL,
    c_ytd_payment NUMERIC(12,2) NOT NULL,
    c_payment_cnt INTEGER NOT NULL,
    c_delivery_cnt INTEGER NOT NULL,
    c_last_order_id INTEGER,
    PRIMARY KEY (c_w_id, c_d_id, c_id)
);

CREATE TABLE IF NOT EXISTS item (
    i_id INTEGER PRIMARY KEY,
    i_name TEXT NOT NULL,
    i_price NUMERIC(6,2) NOT NULL
);

CREATE TABLE IF NOT EXISTS stock (
    s_i_id INTEGER NOT NULL,
    s_w_id INTEGER NOT NULL,
    s_quantity INTEGER NOT NULL,
    s_ytd INTEGER NOT NULL,
    s_order_cnt INTEGER NOT NULL,
    s_remote_cnt INTEGER NOT NULL,
    PRIMARY KEY (s_w_id, s_i_id)
);

CREATE TABLE IF NOT EXISTS orders (
    o_id INTEGER NOT NULL,
    o_d_id INTEGER NOT NULL,
    o_w_id INTEGER NOT NULL,
    o_c_id INTEGER NOT NULL,
    o_entry_d TEXT NOT NULL,
    o_carrier_id INTEGER,
    o_ol_cnt INTEGER NOT NULL,
    o_all_local INTEGER NOT NULL,
    o_status TEXT NOT NULL,
    PRIMARY KEY (o_w_id, o_d_id, o_id)
);

CREATE TABLE IF NOT EXISTS new_order (
    no_o_id INTEGER NOT NULL,
    no_d_id INTEGER NOT NULL,
    no_w_id INTEGER NOT NULL,
    PRIMARY KEY (no_w_id, no_d_id, no_o_id)
);

CREATE TABLE IF NOT EXISTS order_line (
    ol_o_id INTEGER NOT NULL,
    ol_d_id INTEGER NOT NULL,
    ol_w_id INTEGER NOT NULL,
    ol_number INTEGER NOT NULL,
    ol_i_id INTEGER NOT NULL,
    ol_supply_w_id INTEGER NOT NULL,
    ol_delivery_d TEXT,
    ol_quantity INTEGER NOT NULL,
    ol_amount NUMERIC(8,2) NOT NULL,
    PRIMARY KEY (ol_w_id, ol_d_id, ol_o_id, ol_number)
);

CREATE TABLE IF NOT EXISTS history (
    h_id VARCHAR(255) PRIMARY KEY,
    h_c_id INTEGER NOT NULL,
    h_c_d_id INTEGER NOT NULL,
    h_c_w_id INTEGER NOT NULL,
    h_d_id INTEGER NOT NULL,
    h_w_id INTEGER NOT NULL,
    h_amount NUMERIC(6,2) NOT NULL,
    h_data TEXT NOT NULL
);
