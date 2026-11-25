#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use ::catalog::contract::schema_class::SchemaClassKind;

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_cache_like_mirrors_table_shape() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        // users(id INT PRIMARY KEY, name VARCHAR): id is a non-nullable key
        // column, name is a nullable value column in the source table.
        let bound = binder()
            .bind(
                parse_stmt("create cache users_cache like users ttl 60;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateCache(create)) = bound else {
            panic!("expected create cache");
        };
        let schema = &create.schema;
        assert_eq!(schema.class_name(), "users_cache");
        assert_eq!(schema.schema(), &DEFAULT_SCHEMA.to_string());
        assert_eq!(schema.kind(), SchemaClassKind::Cache);
        // The cache mirrors the source table's key/value split: one key
        // column (id) and one value column (name).
        assert_eq!(schema.key_indices(), &vec![0]);
        assert_eq!(schema.value_indices(), &vec![1]);
        assert_eq!(schema.columns().len(), 2);
        let id = &schema.columns()[0];
        assert_eq!(id.type_id(), TypeFamily::I32);
        assert!(!id.nullable(), "key nullability is inherited");
        let name = &schema.columns()[1];
        assert_eq!(name.type_id(), TypeFamily::String);
        assert!(name.nullable(), "value nullability is inherited");
        assert_eq!(create.ttl_secs, Some(60));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_cache_without_ttl_is_unbounded() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("create cache users_cache like users;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateCache(create)) = bound else {
            panic!("expected create cache");
        };
        assert_eq!(create.ttl_secs, None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_cache_like_missing_table_fails() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("create cache c like no_such_table;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    })
    .unwrap()
}
