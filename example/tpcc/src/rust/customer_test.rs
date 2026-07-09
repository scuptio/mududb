//! Unit tests for the `rust::customer::Customer` entity.

use mududb::contract::database::entity::Entity;
use mududb::types::data_value::DataValue;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::customer::object::Customer;

#[test]
fn customer_lifecycle() {
    let c_id_sample = 1i32;
    let c_d_id_sample = 1i32;
    let c_w_id_sample = 1i32;
    let c_first_sample = "c_first_val".to_string();
    let c_last_sample = "c_last_val".to_string();
    let c_discount_sample = 1i32;
    let c_credit_sample = "c_credit_val".to_string();
    let c_balance_sample = 1i32;
    let c_ytd_payment_sample = 1i32;
    let c_payment_cnt_sample = 1i32;
    let c_delivery_cnt_sample = 1i32;
    let c_last_order_id_sample = 1i32;
    let mut entity = Customer::new_empty();
    entity.set_c_id(c_id_sample);
    entity.set_c_d_id(c_d_id_sample);
    entity.set_c_w_id(c_w_id_sample);
    entity.set_c_first(c_first_sample.clone());
    entity.set_c_last(c_last_sample.clone());
    entity.set_c_discount(c_discount_sample);
    entity.set_c_credit(c_credit_sample.clone());
    entity.set_c_balance(c_balance_sample);
    entity.set_c_ytd_payment(c_ytd_payment_sample);
    entity.set_c_payment_cnt(c_payment_cnt_sample);
    entity.set_c_delivery_cnt(c_delivery_cnt_sample);
    entity.set_c_last_order_id(c_last_order_id_sample);
    assert_eq!(entity.get_c_id(), &Some(c_id_sample));
    assert_eq!(entity.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(entity.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(entity.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(entity.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(entity.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(entity.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(entity.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(entity.get_c_ytd_payment(), &Some(c_ytd_payment_sample));
    assert_eq!(entity.get_c_payment_cnt(), &Some(c_payment_cnt_sample));
    assert_eq!(entity.get_c_delivery_cnt(), &Some(c_delivery_cnt_sample));
    assert_eq!(entity.get_c_last_order_id(), &Some(c_last_order_id_sample));

    // Entity metadata
    assert_eq!(Customer::object_name(), "customer");
    assert_eq!(Customer::tuple_desc().fields().len(), 12);

    // Datum / DatumDyn metadata
    let data_type = Customer::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Customer::from_tuple(&tuple).unwrap();
    assert_eq!(from_tuple.get_c_id(), &Some(c_id_sample));
    assert_eq!(from_tuple.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(from_tuple.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(from_tuple.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(from_tuple.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(from_tuple.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(from_tuple.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(from_tuple.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(from_tuple.get_c_ytd_payment(), &Some(c_ytd_payment_sample));
    assert_eq!(from_tuple.get_c_payment_cnt(), &Some(c_payment_cnt_sample));
    assert_eq!(
        from_tuple.get_c_delivery_cnt(),
        &Some(c_delivery_cnt_sample)
    );
    assert_eq!(
        from_tuple.get_c_last_order_id(),
        &Some(c_last_order_id_sample)
    );

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = Customer::from_value(&value).unwrap();
    assert_eq!(from_value.get_c_id(), &Some(c_id_sample));
    assert_eq!(from_value.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(from_value.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(from_value.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(from_value.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(from_value.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(from_value.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(from_value.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(from_value.get_c_ytd_payment(), &Some(c_ytd_payment_sample));
    assert_eq!(from_value.get_c_payment_cnt(), &Some(c_payment_cnt_sample));
    assert_eq!(
        from_value.get_c_delivery_cnt(),
        &Some(c_delivery_cnt_sample)
    );
    assert_eq!(
        from_value.get_c_last_order_id(),
        &Some(c_last_order_id_sample)
    );

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = Customer::from_binary(binary.as_ref()).unwrap();
    assert_eq!(from_binary.get_c_id(), &Some(c_id_sample));
    assert_eq!(from_binary.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(from_binary.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(from_binary.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(from_binary.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(from_binary.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(from_binary.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(from_binary.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(from_binary.get_c_ytd_payment(), &Some(c_ytd_payment_sample));
    assert_eq!(from_binary.get_c_payment_cnt(), &Some(c_payment_cnt_sample));
    assert_eq!(
        from_binary.get_c_delivery_cnt(),
        &Some(c_delivery_cnt_sample)
    );
    assert_eq!(
        from_binary.get_c_last_order_id(),
        &Some(c_last_order_id_sample)
    );

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = Customer::from_textual(textual.as_str()).unwrap();
    assert_eq!(from_textual.get_c_id(), &Some(c_id_sample));
    assert_eq!(from_textual.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(from_textual.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(from_textual.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(from_textual.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(from_textual.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(from_textual.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(from_textual.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(
        from_textual.get_c_ytd_payment(),
        &Some(c_ytd_payment_sample)
    );
    assert_eq!(
        from_textual.get_c_payment_cnt(),
        &Some(c_payment_cnt_sample)
    );
    assert_eq!(
        from_textual.get_c_delivery_cnt(),
        &Some(c_delivery_cnt_sample)
    );
    assert_eq!(
        from_textual.get_c_last_order_id(),
        &Some(c_last_order_id_sample)
    );

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Customer::from_value(&cloned_value).unwrap();
    assert_eq!(from_cloned.get_c_id(), &Some(c_id_sample));
    assert_eq!(from_cloned.get_c_d_id(), &Some(c_d_id_sample));
    assert_eq!(from_cloned.get_c_w_id(), &Some(c_w_id_sample));
    assert_eq!(from_cloned.get_c_first(), &Some(c_first_sample.clone()));
    assert_eq!(from_cloned.get_c_last(), &Some(c_last_sample.clone()));
    assert_eq!(from_cloned.get_c_discount(), &Some(c_discount_sample));
    assert_eq!(from_cloned.get_c_credit(), &Some(c_credit_sample.clone()));
    assert_eq!(from_cloned.get_c_balance(), &Some(c_balance_sample));
    assert_eq!(from_cloned.get_c_ytd_payment(), &Some(c_ytd_payment_sample));
    assert_eq!(from_cloned.get_c_payment_cnt(), &Some(c_payment_cnt_sample));
    assert_eq!(
        from_cloned.get_c_delivery_cnt(),
        &Some(c_delivery_cnt_sample)
    );
    assert_eq!(
        from_cloned.get_c_last_order_id(),
        &Some(c_last_order_id_sample)
    );

    // Field-level binary/value accessors
    {
        let bin = entity.get_field_binary("c_id").unwrap().unwrap();
        entity.set_field_binary("c_id", &bin).unwrap();
        let val = entity.get_field_value("c_id").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_id_sample);
        entity
            .set_field_value("c_id", DataValue::from_i32(c_id_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_id")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_id_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_d_id").unwrap().unwrap();
        entity.set_field_binary("c_d_id", &bin).unwrap();
        let val = entity.get_field_value("c_d_id").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_d_id_sample);
        entity
            .set_field_value("c_d_id", DataValue::from_i32(c_d_id_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_d_id")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_d_id_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_w_id").unwrap().unwrap();
        entity.set_field_binary("c_w_id", &bin).unwrap();
        let val = entity.get_field_value("c_w_id").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_w_id_sample);
        entity
            .set_field_value("c_w_id", DataValue::from_i32(c_w_id_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_w_id")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_w_id_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_first").unwrap().unwrap();
        entity.set_field_binary("c_first", &bin).unwrap();
        let val = entity.get_field_value("c_first").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &c_first_sample);
        entity
            .set_field_value("c_first", DataValue::from_string(c_first_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_first")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &c_first_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_last").unwrap().unwrap();
        entity.set_field_binary("c_last", &bin).unwrap();
        let val = entity.get_field_value("c_last").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &c_last_sample);
        entity
            .set_field_value("c_last", DataValue::from_string(c_last_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_last")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &c_last_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_discount").unwrap().unwrap();
        entity.set_field_binary("c_discount", &bin).unwrap();
        let val = entity.get_field_value("c_discount").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_discount_sample);
        entity
            .set_field_value("c_discount", DataValue::from_i32(c_discount_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_discount")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_discount_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_credit").unwrap().unwrap();
        entity.set_field_binary("c_credit", &bin).unwrap();
        let val = entity.get_field_value("c_credit").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &c_credit_sample);
        entity
            .set_field_value("c_credit", DataValue::from_string(c_credit_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_credit")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &c_credit_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_balance").unwrap().unwrap();
        entity.set_field_binary("c_balance", &bin).unwrap();
        let val = entity.get_field_value("c_balance").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_balance_sample);
        entity
            .set_field_value("c_balance", DataValue::from_i32(c_balance_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_balance")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_balance_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_ytd_payment").unwrap().unwrap();
        entity.set_field_binary("c_ytd_payment", &bin).unwrap();
        let val = entity.get_field_value("c_ytd_payment").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_ytd_payment_sample);
        entity
            .set_field_value("c_ytd_payment", DataValue::from_i32(c_ytd_payment_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_ytd_payment")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_ytd_payment_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_payment_cnt").unwrap().unwrap();
        entity.set_field_binary("c_payment_cnt", &bin).unwrap();
        let val = entity.get_field_value("c_payment_cnt").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_payment_cnt_sample);
        entity
            .set_field_value("c_payment_cnt", DataValue::from_i32(c_payment_cnt_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_payment_cnt")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_payment_cnt_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_delivery_cnt").unwrap().unwrap();
        entity.set_field_binary("c_delivery_cnt", &bin).unwrap();
        let val = entity.get_field_value("c_delivery_cnt").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_delivery_cnt_sample);
        entity
            .set_field_value("c_delivery_cnt", DataValue::from_i32(c_delivery_cnt_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_delivery_cnt")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_delivery_cnt_sample
        );
    }
    {
        let bin = entity.get_field_binary("c_last_order_id").unwrap().unwrap();
        entity.set_field_binary("c_last_order_id", &bin).unwrap();
        let val = entity.get_field_value("c_last_order_id").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &c_last_order_id_sample);
        entity
            .set_field_value(
                "c_last_order_id",
                DataValue::from_i32(c_last_order_id_sample),
            )
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("c_last_order_id")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &c_last_order_id_sample
        );
    }
}
