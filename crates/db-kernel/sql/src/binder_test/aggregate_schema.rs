#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

fn bind_select(sql: &'static str) -> crate::bound_stmt::BoundSelect {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let bound = binder().bind(parse_stmt(sql), &()).await.unwrap();
        let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
            panic!("expected bound select");
        };
        select
    })
    .unwrap()
}

fn bind_select_err(sql: &'static str) -> ErrorCode {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        binder().bind(parse_stmt(sql), &()).await.unwrap_err().ec()
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_aggregate_count_star() {
    let select = bind_select("select count(*) as c from users;");
    assert_eq!(select.select_items.len(), 1);
    let BoundSelectItem::Aggregate(aggregate) = &select.select_items[0] else {
        panic!("expected aggregate select item");
    };
    assert_eq!(aggregate.func, AggregateFunc::Count);
    assert_eq!(aggregate.arg, None);
    assert_eq!(aggregate.result_type.type_family(), TypeFamily::I64);
    assert_eq!(aggregate.output_name, "c");
    assert!(!aggregate.nullable);
    assert_eq!(select.tuple_desc.fields()[0].name(), "c");
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_aggregate_result_types() {
    let select = bind_select("select sum(id), avg(id), min(name), max(id) from users;");
    let families: Vec<TypeFamily> = select
        .select_items
        .iter()
        .map(|item| match item {
            BoundSelectItem::Aggregate(aggregate) => aggregate.result_type.type_family(),
            BoundSelectItem::Column(_) => panic!("expected aggregate"),
        })
        .collect();
    assert_eq!(
        families,
        vec![
            TypeFamily::I64,
            TypeFamily::Numeric,
            TypeFamily::String,
            TypeFamily::I32
        ]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_mixed_column_and_aggregate_rejected() {
    assert_eq!(
        bind_select_err("select id, count(*) from users;"),
        ErrorCode::NotImplemented
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_unsupported_function_rejected() {
    assert_eq!(
        bind_select_err("select foo(id) from users;"),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        bind_select_err("select sum(name) from users;"),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        bind_select_err("select sum(*) from users;"),
        ErrorCode::NotImplemented
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_non_key_predicate_becomes_residual() {
    let select = bind_select("select id from users where name = 'x';");
    assert_eq!(select.residual.len(), 1);
    assert_eq!(select.residual[0].attr, 1);
    assert_eq!(select.residual[0].op, ValueCompare::EQ);
    assert!(matches!(
        select.predicate,
        BoundPredicate::KeyRange { .. } | BoundPredicate::True
    ));
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_key_predicate_plus_residual() {
    let select = bind_select("select id from users where id = 1 and name = 'x';");
    assert!(matches!(select.predicate, BoundPredicate::KeyEq { .. }));
    assert_eq!(select.residual.len(), 1);
    assert_eq!(select.residual[0].attr, 1);
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_aggregate_with_residual() {
    let select = bind_select("select count(*) from users where id = 1 and name = 'x';");
    assert!(matches!(select.predicate, BoundPredicate::KeyEq { .. }));
    assert_eq!(select.residual.len(), 1);
    assert!(matches!(
        select.select_items[0],
        BoundSelectItem::Aggregate(_)
    ));
}

/// Timing probe for the statement-path cost discussion: how expensive is
/// `Binder::bind` for TPC-C-shaped statements (literals, no params).
/// Not a correctness test; prints per-bind cost.
#[test]
#[cfg_attr(miri, ignore)]
fn bind_bench() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let binder = binder_arc(Arc::new(TestMetaMgr::new(composite_schema())));
        let sqls = [
            "SELECT tenant_id, user_id, name FROM accounts WHERE tenant_id = 1 AND user_id = 2",
            "UPDATE accounts SET name = 'x' WHERE tenant_id = 1 AND user_id = 2",
            "INSERT INTO accounts (tenant_id, user_id, name) VALUES (1, 2, 'x')",
            "DELETE FROM accounts WHERE tenant_id = 1 AND user_id = 2",
        ];
        let parsed: Vec<StmtType> = sqls.iter().map(|sql| parse_stmt(sql)).collect();
        let iters = 20_000usize;
        let start = mudu_sys::time::instant_now();
        let mut bound_count = 0usize;
        for i in 0..iters {
            let stmt = parsed[i % parsed.len()].clone();
            let _bound = binder.bind(stmt, &()).await.unwrap();
            bound_count += 1;
        }
        let elapsed = start.elapsed();
        assert_eq!(bound_count, iters);
        println!(
            "bind_bench: {} binds in {:?} => {:.2} us/bind",
            iters,
            elapsed,
            elapsed.as_secs_f64() * 1e6 / iters as f64
        );
    })
    .unwrap()
}

fn users_schema_in(schema_ns: &str) -> SchemaTable {
    let mut schema = schema();
    schema.set_schema(schema_ns.to_string());
    schema
}

fn bound_select_table_id(bound: BoundStmt) -> OID {
    let BoundStmt::Query(BoundQuery::Select(select)) = bound else {
        panic!("expected bound select");
    };
    select.table_id
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_resolves_unqualified_in_current_schema() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let default_users = users_schema_in(DEFAULT_SCHEMA);
        let shop_users = users_schema_in("shop");
        let mgr = TestMetaMgr::new(default_users);
        mgr.create_table(&shop_users).await.unwrap();
        let binder = binder_arc_with_schema(Arc::new(mgr), "shop");

        let bound = binder
            .bind(parse_stmt("select id from users where id = 1;"), &())
            .await
            .unwrap();
        assert_eq!(bound_select_table_id(bound), shop_users.id());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_qualified_reference_bypasses_current_schema() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let default_users = users_schema_in(DEFAULT_SCHEMA);
        let default_oid = default_users.id();
        let shop_users = users_schema_in("shop");
        let mgr = TestMetaMgr::new(default_users);
        mgr.create_table(&shop_users).await.unwrap();
        let binder = binder_arc_with_schema(Arc::new(mgr), "shop");

        let bound = binder
            .bind(parse_stmt("select id from mududb.users where id = 1;"), &())
            .await
            .unwrap();
        assert_eq!(bound_select_table_id(bound), default_oid);
        let bound = binder
            .bind(parse_stmt("select id from shop.users where id = 1;"), &())
            .await
            .unwrap();
        assert_eq!(bound_select_table_id(bound), shop_users.id());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_select_unqualified_falls_back_to_default_schema() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let default_users = users_schema_in(DEFAULT_SCHEMA);
        let default_oid = default_users.id();
        let mgr = TestMetaMgr::new(default_users);
        let binder = binder_arc_with_schema(Arc::new(mgr), "shop");

        // `shop` owns no `users`: the search path falls back to the
        // default schema.
        let bound = binder
            .bind(parse_stmt("select id from users where id = 1;"), &())
            .await
            .unwrap();
        assert_eq!(bound_select_table_id(bound), default_oid);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_create_table_lands_in_current_schema() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let binder = binder_arc_with_schema(Arc::new(TestMetaMgr::new(schema())), "shop");
        let bound = binder
            .bind(
                parse_stmt("create table orders (id int primary key, v int);"),
                &(),
            )
            .await
            .unwrap();
        let BoundStmt::Command(BoundCommand::CreateTable(create)) = bound else {
            panic!("expected create table");
        };
        assert_eq!(create.schema.schema(), "shop");
        assert_eq!(create.schema.table_name(), "orders");
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_insert_resolves_qualified_table_exactly() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let default_users = users_schema_in(DEFAULT_SCHEMA);
        let default_oid = default_users.id();
        let mgr = TestMetaMgr::new(default_users);
        let binder = binder_arc_with_schema(Arc::new(mgr), "shop");

        // Qualified reference hits the default-schema table exactly even
        // though the current schema is `shop`.
        let bound = binder
            .bind(
                parse_stmt("insert into mududb.users values (1, 'alice');"),
                &(),
            )
            .await
            .unwrap();
        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.table_id, default_oid);

        // Unqualified resolves through the fallback to the same table.
        let bound = binder
            .bind(parse_stmt("insert into users values (1, 'alice');"), &())
            .await
            .unwrap();
        let BoundStmt::Command(BoundCommand::Insert(insert)) = bound else {
            panic!("expected bound insert");
        };
        assert_eq!(insert.table_id, default_oid);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn bind_drop_table_qualified_miss_does_not_fall_back() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let default_users = users_schema_in(DEFAULT_SCHEMA);
        let default_oid = default_users.id();
        let mgr = TestMetaMgr::new(default_users);
        let binder = binder_arc_with_schema(Arc::new(mgr), "shop");

        // `shop.users` does not exist; a qualified DROP must not fall
        // back to the default-schema table.
        let err = binder
            .bind(parse_stmt("drop table shop.users;"), &())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        let bound = binder
            .bind(parse_stmt("drop table if exists shop.users;"), &())
            .await
            .unwrap();
        let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
            panic!("expected bound drop table");
        };
        assert!(drop.oid.is_none());

        // Unqualified DROP follows the search path and finds the
        // default-schema table.
        let bound = binder
            .bind(parse_stmt("drop table users;"), &())
            .await
            .unwrap();
        let BoundStmt::Command(BoundCommand::DropTable(drop)) = bound else {
            panic!("expected bound drop table");
        };
        assert_eq!(drop.oid, Some(default_oid));
    })
    .unwrap()
}
