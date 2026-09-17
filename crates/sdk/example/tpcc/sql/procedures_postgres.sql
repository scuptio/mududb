-- PostgreSQL (PL/pgSQL) stored-procedure variants of the TPC-C benchmark
-- transactions, ported 1:1 from the Rust sync procedures in
-- src/rust/procedure.rs and src/rust/seckill.rs.
--
-- Installed by the tpcc-benchmark client itself (--mode pg-procedure, see
-- src/bin/tpcc_benchmark.rs) against the database named by MUDU_CONNECTION;
-- used by the `postgres-procedure` backend of bench_cross_db.py.
--
-- Parameter names carry a p_ prefix and locals a v_ prefix so they can never
-- collide with table column names inside the SQL statements below.

CREATE OR REPLACE FUNCTION tpcc_new_order(
    p_w_id INTEGER,
    p_d_id INTEGER,
    p_c_id INTEGER,
    p_item_ids TEXT,
    p_supplier_w_ids TEXT,
    p_quantities TEXT
) RETURNS TEXT AS $$
DECLARE
    v_item_ids INTEGER[] := string_to_array(p_item_ids, ',')::INTEGER[];
    v_supplier_w_ids INTEGER[] := string_to_array(p_supplier_w_ids, ',')::INTEGER[];
    v_quantities INTEGER[] := string_to_array(p_quantities, ',')::INTEGER[];
    v_line_count INTEGER;
    v_d_tax NUMERIC;
    v_c_balance NUMERIC;
    v_next_order_id INTEGER;
    v_all_local BOOLEAN;
    v_idx INTEGER;
    v_item_id INTEGER;
    v_supply_w_id INTEGER;
    v_quantity INTEGER;
    v_item_price NUMERIC;
    v_amount NUMERIC;
    v_total_quantity INTEGER := 0;
    v_total_amount NUMERIC := 0;
BEGIN
    IF p_w_id <= 0 OR p_d_id <= 0 OR p_c_id <= 0 THEN
        RAISE EXCEPTION 'warehouse_id, district_id and customer_id must be positive';
    END IF;
    v_line_count := coalesce(array_length(v_item_ids, 1), 0);
    IF v_line_count = 0 THEN
        RAISE EXCEPTION 'new_order requires at least one item';
    END IF;
    IF coalesce(array_length(v_supplier_w_ids, 1), 0) <> v_line_count
        OR coalesce(array_length(v_quantities, 1), 0) <> v_line_count THEN
        RAISE EXCEPTION 'item_ids, supplier_w_ids and quantities length mismatch';
    END IF;

    -- Plain point reads of the district and customer rows (also validates
    -- that both rows exist).
    SELECT d_tax INTO STRICT v_d_tax
        FROM district WHERE d_w_id = p_w_id AND d_id = p_d_id;
    SELECT c_balance INTO STRICT v_c_balance
        FROM customer WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = p_c_id;

    -- Allocate the next order id with an atomic increment under the district
    -- row lock; the allocated id is the pre-increment value.
    UPDATE district SET d_next_o_id = d_next_o_id + 1
        WHERE d_w_id = p_w_id AND d_id = p_d_id
        RETURNING d_next_o_id - 1 INTO STRICT v_next_order_id;

    v_all_local := TRUE;
    FOR v_idx IN 1..v_line_count LOOP
        IF v_supplier_w_ids[v_idx] <> p_w_id THEN
            v_all_local := FALSE;
            EXIT;
        END IF;
    END LOOP;

    INSERT INTO orders (o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status)
        VALUES (v_next_order_id, p_d_id, p_w_id, p_c_id,
                'pg-o' || v_next_order_id, 0, v_line_count,
                CASE WHEN v_all_local THEN 1 ELSE 0 END, 'NEW');
    INSERT INTO new_order (no_o_id, no_d_id, no_w_id)
        VALUES (v_next_order_id, p_d_id, p_w_id);

    FOR v_idx IN 1..v_line_count LOOP
        v_item_id := v_item_ids[v_idx];
        v_supply_w_id := v_supplier_w_ids[v_idx];
        v_quantity := v_quantities[v_idx];
        IF v_item_id <= 0 OR v_supply_w_id <= 0 OR v_quantity <= 0 THEN
            RAISE EXCEPTION 'item_id, supplier_warehouse_id and quantity must be positive';
        END IF;

        SELECT i_price INTO STRICT v_item_price
            FROM item WHERE i_id = v_item_id;
        v_amount := v_item_price * v_quantity;

        -- Conditional restock `((current - 10 - q) mod 91) + 10` plus the
        -- commuting ytd/order_cnt/remote_cnt increments.
        UPDATE stock SET
            s_quantity = mod(s_quantity - 10 - v_quantity, 91) + 10,
            s_ytd = s_ytd + v_quantity,
            s_order_cnt = s_order_cnt + 1,
            s_remote_cnt = s_remote_cnt + CASE WHEN v_supply_w_id <> p_w_id THEN 1 ELSE 0 END
            WHERE s_w_id = v_supply_w_id AND s_i_id = v_item_id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'stock row not found: s_w_id=% s_i_id=%', v_supply_w_id, v_item_id;
        END IF;

        INSERT INTO order_line (ol_o_id, ol_d_id, ol_w_id, ol_number, ol_i_id,
                                ol_supply_w_id, ol_delivery_d, ol_quantity, ol_amount)
            VALUES (v_next_order_id, p_d_id, p_w_id, v_idx, v_item_id,
                    v_supply_w_id, '', v_quantity, v_amount);

        v_total_quantity := v_total_quantity + v_quantity;
        v_total_amount := v_total_amount + v_amount;
    END LOOP;

    UPDATE customer SET c_last_order_id = v_next_order_id
        WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = p_c_id;

    RETURN 'order=' || v_next_order_id
        || ';lines=' || v_line_count
        || ';qty=' || v_total_quantity
        || ';amount=' || trunc(v_total_amount)::BIGINT
        || ';all_local=' || v_all_local
        || ';status=NEW';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION tpcc_payment(
    p_w_id INTEGER,
    p_d_id INTEGER,
    p_c_id INTEGER,
    p_amount INTEGER
) RETURNS INTEGER AS $$
DECLARE
    v_c_balance NUMERIC;
BEGIN
    IF p_w_id <= 0 OR p_d_id <= 0 OR p_c_id <= 0 OR p_amount <= 0 THEN
        RAISE EXCEPTION 'warehouse_id, district_id, customer_id and amount must be positive';
    END IF;

    SELECT c_balance INTO STRICT v_c_balance
        FROM customer WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = p_c_id;

    INSERT INTO history (h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_w_id, h_amount, h_data)
        VALUES (gen_random_uuid()::TEXT, p_c_id, p_d_id, p_w_id, p_d_id, p_w_id,
                p_amount, 'payment warehouse=' || p_w_id || ' district=' || p_d_id);

    UPDATE customer SET
        c_balance = c_balance - p_amount,
        c_ytd_payment = c_ytd_payment + p_amount,
        c_payment_cnt = c_payment_cnt + 1
        WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = p_c_id;
    UPDATE district SET d_ytd = d_ytd + p_amount
        WHERE d_w_id = p_w_id AND d_id = p_d_id;
    UPDATE warehouse SET w_ytd = w_ytd + p_amount
        WHERE w_id = p_w_id;

    RETURN trunc(v_c_balance - p_amount)::INTEGER;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION tpcc_order_status(
    p_w_id INTEGER,
    p_d_id INTEGER,
    p_c_id INTEGER
) RETURNS TEXT AS $$
DECLARE
    v_order_id INTEGER;
    v_status TEXT;
BEGIN
    IF p_w_id <= 0 OR p_d_id <= 0 OR p_c_id <= 0 THEN
        RAISE EXCEPTION 'warehouse_id, district_id and customer_id must be positive';
    END IF;

    SELECT c_last_order_id INTO STRICT v_order_id
        FROM customer WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = p_c_id;
    IF v_order_id IS NULL THEN
        RAISE EXCEPTION 'customer.c_last_order_id is null';
    END IF;

    -- STRICT raises NO_DATA_FOUND when the referenced order does not exist,
    -- aborting the transaction exactly like the Rust procedure does.
    SELECT o_status INTO STRICT v_status
        FROM orders WHERE o_w_id = p_w_id AND o_d_id = p_d_id AND o_id = v_order_id;
    RETURN v_status;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION tpcc_delivery(
    p_w_id INTEGER,
    p_d_id INTEGER,
    p_carrier_id INTEGER
) RETURNS TEXT AS $$
DECLARE
    v_order_id INTEGER;
    v_customer_id INTEGER;
BEGIN
    IF p_w_id <= 0 OR p_d_id <= 0 OR p_carrier_id <= 0 THEN
        RAISE EXCEPTION 'warehouse_id, district_id and carrier_id must be positive';
    END IF;

    SELECT min(no_o_id) INTO v_order_id
        FROM new_order WHERE no_w_id = p_w_id AND no_d_id = p_d_id;
    IF v_order_id IS NULL THEN
        RAISE EXCEPTION 'delivery found no pending new_order rows';
    END IF;

    DELETE FROM new_order
        WHERE no_w_id = p_w_id AND no_d_id = p_d_id AND no_o_id = v_order_id;
    UPDATE district SET d_last_delivery_o_id = v_order_id
        WHERE d_w_id = p_w_id AND d_id = p_d_id;
    UPDATE orders SET o_carrier_id = p_carrier_id, o_status = 'DELIVERED'
        WHERE o_w_id = p_w_id AND o_d_id = p_d_id AND o_id = v_order_id;

    SELECT o_c_id INTO STRICT v_customer_id
        FROM orders WHERE o_w_id = p_w_id AND o_d_id = p_d_id AND o_id = v_order_id;
    UPDATE customer SET c_delivery_cnt = c_delivery_cnt + 1
        WHERE c_w_id = p_w_id AND c_d_id = p_d_id AND c_id = v_customer_id;

    RETURN 'delivered order=' || v_order_id || ' carrier=' || p_carrier_id;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION tpcc_stock_level(
    p_w_id INTEGER,
    p_d_id INTEGER,
    p_threshold INTEGER
) RETURNS INTEGER AS $$
DECLARE
    v_count BIGINT;
BEGIN
    IF p_w_id <= 0 OR p_d_id <= 0 OR p_threshold <= 0 THEN
        RAISE EXCEPTION 'warehouse_id, district_id and threshold must be positive';
    END IF;

    SELECT count(*) INTO v_count
        FROM stock WHERE s_w_id = p_w_id AND s_quantity < p_threshold;
    RETURN v_count::INTEGER;
END;
$$ LANGUAGE plpgsql;

-- Hot-row contention injector counterpart (tpcc_hotspot is created
-- client-side by the benchmark when hot rows are enabled).
CREATE OR REPLACE FUNCTION tpcc_hotspot_hit(
    p_w_id INTEGER,
    p_hot_id INTEGER
) RETURNS INTEGER AS $$
DECLARE
    v_counter INTEGER;
BEGIN
    IF p_w_id <= 0 OR p_hot_id <= 0 THEN
        RAISE EXCEPTION 'warehouse_id and hot_id must be positive';
    END IF;

    UPDATE tpcc_hotspot SET h_counter = h_counter + 1
        WHERE h_w_id = p_w_id AND h_id = p_hot_id;
    SELECT h_counter INTO STRICT v_counter
        FROM tpcc_hotspot WHERE h_w_id = p_w_id AND h_id = p_hot_id;
    RETURN v_counter;
END;
$$ LANGUAGE plpgsql;

-- Seckill (flash-sale) buy; returns 'ok' on success and 'sold_out' when the
-- item has no stock left (not an abort, matching the Rust procedure).
CREATE OR REPLACE FUNCTION seckill_buy(
    p_item_id INTEGER,
    p_order_id INTEGER,
    p_user_id INTEGER,
    p_amount INTEGER,
    p_payload TEXT
) RETURNS TEXT AS $$
DECLARE
    v_stock INTEGER;
BEGIN
    IF p_item_id <= 0 OR p_order_id <= 0 OR p_user_id <= 0 OR p_amount <= 0 THEN
        RAISE EXCEPTION 'item_id, order_id, user_id and amount must be positive';
    END IF;

    SELECT si_stock INTO v_stock
        FROM seckill_item WHERE si_id = p_item_id;
    IF v_stock IS NOT NULL AND v_stock > 0 THEN
        UPDATE seckill_item SET si_stock = si_stock - 1, si_sold = si_sold + 1
            WHERE si_id = p_item_id;
        INSERT INTO seckill_order (so_item_id, so_id, so_user_id, so_amount, so_payload)
            VALUES (p_item_id, p_order_id, p_user_id, p_amount, p_payload);
        RETURN 'ok';
    END IF;
    RETURN 'sold_out';
END;
$$ LANGUAGE plpgsql;
