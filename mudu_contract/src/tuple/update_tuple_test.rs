#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::slot::Slot;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use crate::tuple::update_tuple::update_tuple;
    use mudu::common::buf::Buf;
    use mudu::common::update_delta::UpdateDelta;
    use mudu::error::ErrorCode;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    fn desc_i32_string() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![
            DatType::new_no_param(DatTypeID::I32),
            DatType::default_for(DatTypeID::String),
        ])
        .unwrap()
    }

    fn desc_two_strings() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![
            DatType::default_for(DatTypeID::String),
            DatType::default_for(DatTypeID::String),
        ])
        .unwrap()
    }

    fn desc_i32() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DatType::new_no_param(DatTypeID::I32)]).unwrap()
    }

    #[test]
    fn update_fixed_len_field() {
        let desc = desc_i32();
        let tuple = build_tuple(&[vec![0, 0, 0, 42]], &desc).unwrap();
        let new_value: Buf = vec![0, 0, 0, 7];
        let mut delta: Vec<UpdateDelta> = Vec::new();

        update_tuple(0, &new_value, &desc, &tuple, &mut delta).unwrap();

        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].offset(), 0);
        assert_eq!(delta[0].to_replace_size(), 4);
        assert_eq!(delta[0].delta(), &new_value);

        let mut applied = tuple.clone();
        for d in &delta {
            d.apply_to(&mut applied);
        }
        assert_eq!(&applied[0..4], &[0, 0, 0, 7]);
    }

    #[test]
    fn update_var_len_field_within_capacity() {
        let desc = desc_i32_string();
        let tuple = build_tuple(&[vec![0, 0, 0, 1], b"hi".to_vec()], &desc).unwrap();
        let new_value: Buf = b"hello".to_vec();
        let mut delta: Vec<UpdateDelta> = Vec::new();

        update_tuple(1, &new_value, &desc, &tuple, &mut delta).unwrap();

        assert_eq!(delta.len(), 2);
        let slot_delta = &delta[0];
        let data_delta = &delta[1];

        assert_eq!(
            slot_delta.offset(),
            desc.get_field_desc(1).slot().offset() as u32
        );
        assert_eq!(slot_delta.to_replace_size(), Slot::size_of());

        let decoded_slot = Slot::from_binary(slot_delta.delta()).unwrap();
        assert_eq!(
            decoded_slot.offset(),
            desc.meta_size() + desc.total_fixed_data_size()
        );
        assert_eq!(decoded_slot.length(), new_value.len());

        assert_eq!(data_delta.offset(), decoded_slot.offset() as u32);
        assert_eq!(data_delta.to_replace_size(), 2); // old "hi" length
        assert_eq!(data_delta.delta(), &new_value);

        let mut applied = tuple.clone();
        for d in &delta {
            d.apply_to(&mut applied);
        }
        let slot = Slot::from_binary(&applied[0..Slot::size_of()]).unwrap();
        assert_eq!(
            &applied[slot.offset()..slot.offset() + slot.length()],
            b"hello"
        );
    }

    #[test]
    fn update_var_len_field_oversized_rewrites_trailing_slots() {
        let desc = desc_two_strings();
        let tuple = build_tuple(&[b"a".to_vec(), b"b".to_vec()], &desc).unwrap();
        let new_value: Buf = vec![b'x'; 50];
        let mut delta: Vec<UpdateDelta> = Vec::new();

        update_tuple(0, &new_value, &desc, &tuple, &mut delta).unwrap();

        assert_eq!(delta.len(), 2);
        let slot_delta = &delta[0];
        let data_delta = &delta[1];

        assert_eq!(
            slot_delta.offset(),
            desc.get_field_desc(0).slot().offset() as u32
        );
        assert_eq!(
            slot_delta.to_replace_size(),
            Slot::size_of() * desc.field_count()
        );

        let slots = slot_delta.delta();
        let first_slot = Slot::from_binary(&slots[0..Slot::size_of()]).unwrap();
        let second_slot = Slot::from_binary(&slots[Slot::size_of()..2 * Slot::size_of()]).unwrap();

        let data_start = desc.meta_size();
        assert_eq!(first_slot.offset(), data_start);
        assert_eq!(first_slot.length(), new_value.len());
        assert_eq!(second_slot.offset(), data_start + new_value.len());
        assert_eq!(second_slot.length(), 1);

        assert_eq!(data_delta.offset(), data_start as u32);
        assert_eq!(data_delta.to_replace_size(), 2); // old "a" + "b"
        let expected_data: Buf = {
            let mut buf = new_value.clone();
            buf.push(b'b');
            buf
        };
        assert_eq!(data_delta.delta(), &expected_data);

        let mut applied = tuple.clone();
        for d in &delta {
            d.apply_to(&mut applied);
        }
        let s0 = Slot::from_binary(&applied[0..Slot::size_of()]).unwrap();
        let s1 = Slot::from_binary(&applied[Slot::size_of()..2 * Slot::size_of()]).unwrap();
        assert_eq!(
            &applied[s0.offset()..s0.offset() + s0.length()],
            &new_value[..]
        );
        assert_eq!(&applied[s1.offset()..s1.offset() + s1.length()], b"b");
    }

    #[test]
    fn update_tuple_rejects_out_of_bounds_index() {
        let desc = desc_i32();
        let tuple = build_tuple(&[vec![0, 0, 0, 1]], &desc).unwrap();
        let mut delta: Vec<UpdateDelta> = Vec::new();

        let err = update_tuple(5, &vec![0, 0, 0, 2], &desc, &tuple, &mut delta).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Internal);
    }
}
