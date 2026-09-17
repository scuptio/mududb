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
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    fn desc_i32_string() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![
            DataType::new_no_param(TypeFamily::I32),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap()
    }

    fn desc_two_strings() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![
            DataType::default_for(TypeFamily::String),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap()
    }

    fn desc_i32() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::I32)]).unwrap()
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
    fn update_var_len_field_grows_last_field() {
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
    fn update_var_len_field_shorter_rewrites_trailing_slots() {
        let desc = desc_two_strings();
        let tuple = build_tuple(&[b"alpha".to_vec(), b"omega".to_vec()], &desc).unwrap();
        let new_value: Buf = b"al".to_vec();
        let mut delta: Vec<UpdateDelta> = Vec::new();

        update_tuple(0, &new_value, &desc, &tuple, &mut delta).unwrap();

        let mut applied = tuple.clone();
        for d in &delta {
            d.apply_to(&mut applied);
        }
        let data_start = desc.meta_size();
        let s0 = Slot::from_binary(&applied[0..Slot::size_of()]).unwrap();
        let s1 = Slot::from_binary(&applied[Slot::size_of()..2 * Slot::size_of()]).unwrap();
        assert_eq!(s0.offset(), data_start);
        assert_eq!(s0.length(), new_value.len());
        // A shorter value on a non-last varlen field must keep the trailing
        // field's slot and payload intact (shifted by the length delta).
        assert_eq!(s1.offset(), data_start + new_value.len());
        assert_eq!(s1.length(), 5);
        assert_eq!(&applied[s0.offset()..s0.offset() + s0.length()], b"al");
        assert_eq!(&applied[s1.offset()..s1.offset() + s1.length()], b"omega");
        assert_eq!(applied.len(), data_start + new_value.len() + 5);
    }

    #[test]
    fn sequential_var_len_updates_apply_against_evolving_tuple() {
        fn apply_one(updated: &mut Vec<u8>, index: usize, value: &[u8], desc: &TupleBinaryDesc) {
            let mut delta: Vec<UpdateDelta> = Vec::new();
            update_tuple(index, &value.to_vec(), desc, updated, &mut delta).unwrap();
            for d in &delta {
                d.apply_to(updated);
            }
        }

        fn field_bytes(desc: &TupleBinaryDesc, tuple: &[u8], index: usize) -> Vec<u8> {
            desc.get_field_desc(index).get(tuple).unwrap().to_vec()
        }

        let desc = desc_two_strings();
        let mut updated = build_tuple(&[b"aaa".to_vec(), b"bbb".to_vec()], &desc).unwrap();

        // grow field 0, then shrink field 1
        apply_one(&mut updated, 0, b"aaaaa", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"aaaaa".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"bbb".to_vec());

        apply_one(&mut updated, 1, b"c", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"aaaaa".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"c".to_vec());

        // shrink field 0 (non-last varlen), then grow field 1
        apply_one(&mut updated, 0, b"z", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"z".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"c".to_vec());

        apply_one(&mut updated, 1, b"cc", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"z".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"cc".to_vec());

        // same-length updates on both fields
        apply_one(&mut updated, 0, b"y", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"y".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"cc".to_vec());

        apply_one(&mut updated, 1, b"dd", &desc);
        assert_eq!(field_bytes(&desc, &updated, 0), b"y".to_vec());
        assert_eq!(field_bytes(&desc, &updated, 1), b"dd".to_vec());

        assert_eq!(updated.len(), desc.meta_size() + 1 + 2);
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
