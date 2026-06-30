//! Unit tests for the `generated::votes::Votes` entity.

use mududb::contract::database::entity::Entity;
use mududb::types::dat_value::DatValue;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::votes::object::Votes;

#[test]
fn votes_lifecycle() {
    let vote_id_sample = "vote_id_val".to_string();
    let creator_id_sample = "creator_id_val".to_string();
    let topic_sample = "topic_val".to_string();
    let vote_type_sample = "vote_type_val".to_string();
    let max_choices_sample = 1i32;
    let end_time_sample = 1i32;
    let visibility_rule_sample = "visibility_rule_val".to_string();
    let mut entity = Votes::new(
        Some(vote_id_sample.clone()),
        Some(creator_id_sample.clone()),
        Some(topic_sample.clone()),
        Some(vote_type_sample.clone()),
        Some(max_choices_sample),
        Some(end_time_sample),
        Some(visibility_rule_sample.clone()),
    );
    assert_eq!(entity.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(entity.get_creator_id(), &Some(creator_id_sample.clone()));
    assert_eq!(entity.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(entity.get_vote_type(), &Some(vote_type_sample.clone()));
    assert_eq!(entity.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(entity.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        entity.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Entity metadata
    assert_eq!(Votes::object_name(), "votes");
    assert_eq!(Votes::tuple_desc().fields().len(), 7);

    // Datum / DatumDyn metadata
    let dat_type = Votes::dat_type();
    assert_eq!(entity.dat_type_id().unwrap(), dat_type.dat_type_id());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Votes::from_tuple(&tuple).unwrap();
    assert_eq!(from_tuple.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(
        from_tuple.get_creator_id(),
        &Some(creator_id_sample.clone())
    );
    assert_eq!(from_tuple.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(from_tuple.get_vote_type(), &Some(vote_type_sample.clone()));
    assert_eq!(from_tuple.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(from_tuple.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        from_tuple.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Value roundtrip
    let value = entity.to_value(&dat_type).unwrap();
    let from_value = Votes::from_value(&value).unwrap();
    assert_eq!(from_value.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(
        from_value.get_creator_id(),
        &Some(creator_id_sample.clone())
    );
    assert_eq!(from_value.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(from_value.get_vote_type(), &Some(vote_type_sample.clone()));
    assert_eq!(from_value.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(from_value.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        from_value.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Binary roundtrip
    let binary = entity.to_binary(&dat_type).unwrap();
    let from_binary = Votes::from_binary(binary.as_ref()).unwrap();
    assert_eq!(from_binary.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(
        from_binary.get_creator_id(),
        &Some(creator_id_sample.clone())
    );
    assert_eq!(from_binary.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(from_binary.get_vote_type(), &Some(vote_type_sample.clone()));
    assert_eq!(from_binary.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(from_binary.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        from_binary.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Textual roundtrip
    let textual = entity.to_textual(&dat_type).unwrap();
    let from_textual = Votes::from_textual(textual.as_str()).unwrap();
    assert_eq!(from_textual.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(
        from_textual.get_creator_id(),
        &Some(creator_id_sample.clone())
    );
    assert_eq!(from_textual.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(
        from_textual.get_vote_type(),
        &Some(vote_type_sample.clone())
    );
    assert_eq!(from_textual.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(from_textual.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        from_textual.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&dat_type).unwrap();
    let from_cloned = Votes::from_value(&cloned_value).unwrap();
    assert_eq!(from_cloned.get_vote_id(), &Some(vote_id_sample.clone()));
    assert_eq!(
        from_cloned.get_creator_id(),
        &Some(creator_id_sample.clone())
    );
    assert_eq!(from_cloned.get_topic(), &Some(topic_sample.clone()));
    assert_eq!(from_cloned.get_vote_type(), &Some(vote_type_sample.clone()));
    assert_eq!(from_cloned.get_max_choices(), &Some(max_choices_sample));
    assert_eq!(from_cloned.get_end_time(), &Some(end_time_sample));
    assert_eq!(
        from_cloned.get_visibility_rule(),
        &Some(visibility_rule_sample.clone())
    );

    // Field-level binary/value accessors
    {
        let bin = entity.get_field_binary("vote_id").unwrap().unwrap();
        entity.set_field_binary("vote_id", &bin).unwrap();
        let val = entity.get_field_value("vote_id").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &vote_id_sample);
        entity
            .set_field_value("vote_id", DatValue::from_string(vote_id_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("vote_id")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &vote_id_sample
        );
    }
    {
        let bin = entity.get_field_binary("creator_id").unwrap().unwrap();
        entity.set_field_binary("creator_id", &bin).unwrap();
        let val = entity.get_field_value("creator_id").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &creator_id_sample);
        entity
            .set_field_value(
                "creator_id",
                DatValue::from_string(creator_id_sample.clone()),
            )
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("creator_id")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &creator_id_sample
        );
    }
    {
        let bin = entity.get_field_binary("topic").unwrap().unwrap();
        entity.set_field_binary("topic", &bin).unwrap();
        let val = entity.get_field_value("topic").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &topic_sample);
        entity
            .set_field_value("topic", DatValue::from_string(topic_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("topic")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &topic_sample
        );
    }
    {
        let bin = entity.get_field_binary("vote_type").unwrap().unwrap();
        entity.set_field_binary("vote_type", &bin).unwrap();
        let val = entity.get_field_value("vote_type").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &vote_type_sample);
        entity
            .set_field_value("vote_type", DatValue::from_string(vote_type_sample.clone()))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("vote_type")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &vote_type_sample
        );
    }
    {
        let bin = entity.get_field_binary("max_choices").unwrap().unwrap();
        entity.set_field_binary("max_choices", &bin).unwrap();
        let val = entity.get_field_value("max_choices").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &max_choices_sample);
        entity
            .set_field_value("max_choices", DatValue::from_i32(max_choices_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("max_choices")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &max_choices_sample
        );
    }
    {
        let bin = entity.get_field_binary("end_time").unwrap().unwrap();
        entity.set_field_binary("end_time", &bin).unwrap();
        let val = entity.get_field_value("end_time").unwrap().unwrap();
        assert_eq!(val.as_i32().unwrap(), &end_time_sample);
        entity
            .set_field_value("end_time", DatValue::from_i32(end_time_sample))
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("end_time")
                .unwrap()
                .unwrap()
                .as_i32()
                .unwrap(),
            &end_time_sample
        );
    }
    {
        let bin = entity.get_field_binary("visibility_rule").unwrap().unwrap();
        entity.set_field_binary("visibility_rule", &bin).unwrap();
        let val = entity.get_field_value("visibility_rule").unwrap().unwrap();
        assert_eq!(val.as_string().unwrap(), &visibility_rule_sample);
        entity
            .set_field_value(
                "visibility_rule",
                DatValue::from_string(visibility_rule_sample.clone()),
            )
            .unwrap();
        assert_eq!(
            entity
                .get_field_value("visibility_rule")
                .unwrap()
                .unwrap()
                .as_string()
                .unwrap(),
            &visibility_rule_sample
        );
    }
}
