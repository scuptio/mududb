CREATE TABLE warehouse (
    w_id INTEGER PRIMARY KEY,
    w_name TEXT NOT NULL,
    w_tax INTEGER NOT NULL,
    w_ytd INTEGER NOT NULL
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (w_id);

CREATE TABLE district (
    d_id INTEGER NOT NULL,
    d_w_id INTEGER NOT NULL,
    d_name TEXT NOT NULL,
    d_tax INTEGER NOT NULL,
    d_ytd INTEGER NOT NULL,
    d_next_o_id INTEGER NOT NULL,
    d_last_delivery_o_id INTEGER NOT NULL,
    PRIMARY KEY (d_w_id, d_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (d_w_id);

CREATE TABLE customer (
    c_id INTEGER NOT NULL,
    c_d_id INTEGER NOT NULL,
    c_w_id INTEGER NOT NULL,
    c_first TEXT NOT NULL,
    c_last TEXT NOT NULL,
    c_discount INTEGER NOT NULL,
    c_credit TEXT NOT NULL,
    c_balance INTEGER NOT NULL,
    c_ytd_payment INTEGER NOT NULL,
    c_payment_cnt INTEGER NOT NULL,
    c_delivery_cnt INTEGER NOT NULL,
    c_last_order_id INTEGER,
    PRIMARY KEY (c_w_id, c_d_id, c_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (c_w_id);

CREATE TABLE item (
    i_w_id INTEGER NOT NULL,
    i_id INTEGER NOT NULL,
    i_name TEXT NOT NULL,
    i_price INTEGER NOT NULL,
    PRIMARY KEY (i_w_id, i_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (i_w_id);

CREATE TABLE stock (
    s_i_id INTEGER NOT NULL,
    s_w_id INTEGER NOT NULL,
    s_quantity INTEGER NOT NULL,
    s_ytd INTEGER NOT NULL,
    s_order_cnt INTEGER NOT NULL,
    s_remote_cnt INTEGER NOT NULL,
    PRIMARY KEY (s_w_id, s_i_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (s_w_id);

CREATE TABLE orders (
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
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (o_w_id);

CREATE TABLE new_order (
    no_o_id INTEGER NOT NULL,
    no_d_id INTEGER NOT NULL,
    no_w_id INTEGER NOT NULL,
    PRIMARY KEY (no_w_id, no_d_id, no_o_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (no_w_id);

CREATE TABLE order_line (
    ol_o_id INTEGER NOT NULL,
    ol_d_id INTEGER NOT NULL,
    ol_w_id INTEGER NOT NULL,
    ol_number INTEGER NOT NULL,
    ol_i_id INTEGER NOT NULL,
    ol_supply_w_id INTEGER NOT NULL,
    ol_delivery_d TEXT,
    ol_quantity INTEGER NOT NULL,
    ol_amount INTEGER NOT NULL,
    PRIMARY KEY (ol_w_id, ol_d_id, ol_o_id, ol_number)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (ol_w_id);

CREATE TABLE history (
    h_w_id INTEGER NOT NULL,
    h_id TEXT NOT NULL,
    h_c_id INTEGER NOT NULL,
    h_c_d_id INTEGER NOT NULL,
    h_c_w_id INTEGER NOT NULL,
    h_d_id INTEGER NOT NULL,
    h_amount INTEGER NOT NULL,
    h_data TEXT NOT NULL,
    PRIMARY KEY (h_w_id, h_id)
)
PARTITION BY GLOBAL RULE r_tpcc_wh
REFERENCES (h_w_id);
