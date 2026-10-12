#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
#[cfg_attr(miri, ignore)]
fn bind_update_marks_null_fs_column_assignment_for_rebind() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let mut photo = SchemaColumn::new(
            "photo".to_string(),
            TypeFamily::U128,
            DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::U128)),
        );
        photo.set_fs_binding(Some(FsColumnBinding::new(7, FsTypeKind::File)));
        let schema = SchemaTable::new(
            "product".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I64,
                    DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I64)),
                ),
                photo,
            ],
            vec![0],
            vec![1],
        );
        let bound = binder_arc(Arc::new(TestMetaMgr::new(schema)))
            .bind(
                parse_stmt("update product set photo = null where id = 1;"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::Update(update)) = bound else {
            panic!("expected bound update");
        };
        assert_eq!(update.value.len(), 1);
        assert_eq!(update.value[0].0, 1);
        assert!(
            matches!(&update.value[0].1, BoundSetValue::Absolute(datum) if datum.is_empty()),
            "assigning NULL to an fs column binds the empty-datum rebind sentinel"
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_table_returns_oid_when_table_exists() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("drop table users;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
            panic!("expected bound drop table");
        };
        assert!(drop.oid.is_some());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_table_if_missing_returns_none() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("drop table if exists missing;"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
            panic!("expected bound drop table");
        };
        assert!(drop.oid.is_none());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_table_fails_when_missing_without_if_exists() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("drop table missing;"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_copy_from_builds_layout_for_known_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("copy users from 'users.csv';"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CopyFrom(copy)) = bound else {
            panic!("expected bound copy from");
        };
        assert_eq!(copy.file_path, "users.csv");
        assert_eq!(copy.key_index, vec![0]);
        assert_eq!(copy.value_index, vec![1]);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_copy_to_builds_layout_for_known_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder()
            .bind(parse_stmt("copy users to 'users.csv';"), &())
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CopyTo(copy)) = bound else {
            panic!("expected bound copy to");
        };
        assert_eq!(copy.file_path, "users.csv");
        assert_eq!(copy.key_indexing, vec![0]);
        assert_eq!(copy.value_indexing, vec![1]);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_copy_from_fails_for_unknown_table() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(parse_stmt("copy missing from 'missing.csv';"), &())
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

fn fs_type_binder(fs_type: FsTypeDesc) -> Binder {
    binder_arc(Arc::new(TestMetaMgr::with_fs_type(schema(), fs_type)))
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_resolves_registered_fs_type_column_to_u128() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = fs_type_binder(FsTypeDesc::new("photo_fs".to_string(), 7, FsTypeKind::File))
            .bind(
                parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, photo photo_fs);"),
                &(),
            )
            .await
            .unwrap();

        let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
            panic!("expected create table");
        };
        let photo = create
            .schema
            .columns()
            .iter()
            .find(|column| column.get_name() == "photo")
            .expect("photo column");
        assert_eq!(photo.type_id(), TypeFamily::U128);
        assert_eq!(
            photo.fs_binding(),
            Some(FsColumnBinding::new(7, FsTypeKind::File))
        );

        let id = create
            .schema
            .columns()
            .iter()
            .find(|column| column.get_name() == "id")
            .expect("id column");
        assert_eq!(id.type_id(), TypeFamily::I32);
        assert_eq!(id.fs_binding(), None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_resolves_directory_fs_type_column() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = fs_type_binder(FsTypeDesc::new(
            "backup_dir".to_string(),
            3,
            FsTypeKind::Directory,
        ))
        .bind(
            parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, backup backup_dir);"),
            &(),
        )
        .await
        .unwrap();

        let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
            panic!("expected create table");
        };
        let backup = create
            .schema
            .columns()
            .iter()
            .find(|column| column.get_name() == "backup")
            .expect("backup column");
        assert_eq!(backup.type_id(), TypeFamily::U128);
        assert_eq!(
            backup.fs_binding(),
            Some(FsColumnBinding::new(3, FsTypeKind::Directory))
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_fails_for_unregistered_type_name() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let err = binder()
            .bind(
                parse_stmt("CREATE TABLE product (id INT PRIMARY KEY, photo photo_fs);"),
                &(),
            )
            .await
            .unwrap_err();

        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(err.to_string().contains("unknown type name photo_fs"));
    })
    .unwrap()
}
