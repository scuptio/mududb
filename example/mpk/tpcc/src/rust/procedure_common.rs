use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;

pub fn require_positive(name: &str, value: i32) -> RS<()> {
    if value <= 0 {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("{name} must be positive, got {value}")
        ));
    }
    Ok(())
}

pub fn customer_name(warehouse_id: i32, district_id: i32, customer_id: i32) -> (String, String) {
    (
        format!("Customer{warehouse_id}_{district_id}_{customer_id}"),
        format!("Last{customer_id}"),
    )
}

pub fn district_name(warehouse_id: i32, district_id: i32) -> String {
    format!("District{warehouse_id}_{district_id}")
}

pub fn warehouse_name(warehouse_id: i32) -> String {
    format!("Warehouse{warehouse_id}")
}

pub fn item_name(item_id: i32) -> String {
    format!("Item{item_id}")
}

pub fn validate_order_lines(
    item_ids: &[i32],
    supplier_warehouse_ids: &[i32],
    quantities: &[i32],
) -> RS<()> {
    if item_ids.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "new_order requires at least one item"
        ));
    }
    if item_ids.len() != supplier_warehouse_ids.len() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "item_ids and supplier_warehouse_ids length mismatch: {} != {}",
                item_ids.len(),
                supplier_warehouse_ids.len()
            )
        ));
    }
    if item_ids.len() != quantities.len() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "item_ids and quantities length mismatch: {} != {}",
                item_ids.len(),
                quantities.len()
            )
        ));
    }
    for &item_id in item_ids {
        require_positive("item_id", item_id)?;
    }
    for &supplier_warehouse_id in supplier_warehouse_ids {
        require_positive("supplier_warehouse_id", supplier_warehouse_id)?;
    }
    for &quantity in quantities {
        require_positive("quantity", quantity)?;
    }
    Ok(())
}

pub fn order_status_text(
    order_id: i32,
    line_count: usize,
    total_quantity: i32,
    total_amount: i32,
    all_local: bool,
    status: &str,
) -> String {
    format!(
        "order={order_id};lines={line_count};qty={total_quantity};amount={total_amount};all_local={all_local};status={status}"
    )
}
