-- Tables created client-side by the benchmark driver (see
-- src/bin/tpcc_benchmark.rs), not by sql/ddl.sql at package install time.
-- The static SQL checker (mgen check-sql) reads this file together with
-- ddl.sql / ddl_warehouse_partitioned.sql so statements against these
-- tables resolve their schema.

CREATE TABLE tpcc_hotspot
(
    h_w_id    INTEGER NOT NULL,
    h_id      INTEGER NOT NULL,
    h_counter INTEGER NOT NULL,
    PRIMARY KEY (h_w_id, h_id)
);

CREATE TABLE seckill_item
(
    si_id    INTEGER PRIMARY KEY,
    si_name  TEXT NOT NULL,
    si_stock INTEGER NOT NULL,
    si_sold  INTEGER NOT NULL,
    si_price INTEGER NOT NULL
);

CREATE TABLE seckill_order
(
    so_item_id INTEGER NOT NULL,
    so_id      INTEGER NOT NULL,
    so_user_id INTEGER NOT NULL,
    so_amount  INTEGER NOT NULL,
    so_payload TEXT NOT NULL,
    PRIMARY KEY (so_item_id, so_id)
);
