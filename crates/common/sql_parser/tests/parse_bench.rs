use sql_parser::ast::parser::SQLParser;

const STATEMENTS: &[&str] = &[
    "SELECT d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id FROM district WHERE d_w_id = ? AND d_id = ?",
    "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
    "UPDATE district SET d_next_o_id = ? WHERE d_w_id = ? AND d_id = ?",
    "INSERT INTO orders (o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status) VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)",
    "SELECT i_id, i_name, i_price FROM item WHERE i_w_id = ? AND i_id = ?",
    "SELECT s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt FROM stock WHERE s_w_id = ? AND s_i_id = ?",
    "UPDATE stock SET s_quantity = ?, s_ytd = ?, s_order_cnt = ?, s_remote_cnt = ? WHERE s_w_id = ? AND s_i_id = ?",
    "UPDATE warehouse SET w_ytd = ? WHERE w_id = ?",
    "INSERT INTO history (h_w_id, h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_amount, h_data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    "DELETE FROM new_order WHERE no_w_id = ? AND no_d_id = ? AND no_o_id = ?",
];

#[cfg_attr(miri, ignore)]
#[test]
fn parse_bench() {
    let parser = SQLParser::new().unwrap();
    let iters = 20_000;
    let start = mudu_sys::time::instant_now();
    let mut total_stmts = 0usize;
    for i in 0..iters {
        let sql = STATEMENTS[i % STATEMENTS.len()];
        let list = parser.parse(sql).unwrap();
        total_stmts += list.into_stmts().len();
    }
    let elapsed = start.elapsed();
    assert_eq!(total_stmts, iters);
    println!(
        "parse_bench: {} parses in {:?} => {:.2} us/parse",
        iters,
        elapsed,
        elapsed.as_secs_f64() * 1e6 / iters as f64
    );
}

#[cfg_attr(miri, ignore)]
#[test]
fn parser_new_bench() {
    let iters = 1_000;
    let start = mudu_sys::time::instant_now();
    for _ in 0..iters {
        let _p = SQLParser::new().unwrap();
    }
    let elapsed = start.elapsed();
    println!(
        "parser_new_bench: {} constructions in {:?} => {:.2} us/new",
        iters,
        elapsed,
        elapsed.as_secs_f64() * 1e6 / iters as f64
    );
}
