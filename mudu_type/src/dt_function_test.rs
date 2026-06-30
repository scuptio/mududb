#[cfg(test)]
mod tests {
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dt_function::{
        input_textual, output_textual, recv_binary, send_binary, value_from_f32, value_from_f64,
        value_from_i32, value_from_i64, value_from_string,
    };

    #[test]
    fn value_from_scalar_primitives() {
        assert_eq!(*value_from_i32(42).unwrap().as_i32().unwrap(), 42);
        assert_eq!(*value_from_i64(42).unwrap().as_i64().unwrap(), 42);
        assert_eq!(*value_from_f32(1.5).unwrap().as_f32().unwrap(), 1.5);
        assert_eq!(*value_from_f64(1.5).unwrap().as_f64().unwrap(), 1.5);
        assert_eq!(
            *value_from_string("hello".to_string())
                .unwrap()
                .as_string()
                .unwrap(),
            "hello"
        );
    }

    #[test]
    fn input_output_textual_roundtrip() {
        let ty = DatType::new_no_param(DatTypeID::I32);
        let value = input_textual("123", &ty).unwrap();
        let textual = output_textual(&value, &ty).unwrap();
        assert_eq!(textual, "123");
    }

    #[test]
    fn send_recv_binary_roundtrip() {
        let ty = DatType::new_no_param(DatTypeID::I64);
        let value = value_from_i64(0x0102_0304_0506_0708).unwrap();
        let bytes = send_binary(&value, &ty).unwrap();
        let restored = recv_binary(&bytes, &ty).unwrap();
        assert_eq!(*restored.as_i64().unwrap(), 0x0102_0304_0506_0708);
    }
}
