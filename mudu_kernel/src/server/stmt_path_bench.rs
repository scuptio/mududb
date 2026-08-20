//! Micro-benchmark decomposing the server-side CPU cost of one point-lookup
//! SELECT / UPDATE statement, bypassing the network entirely. It times each
//! layer of the statement path (parse cache hit, bind, plan, executor,
//! storage read, index/version-chain/value-file, response encode) so
//! optimization work can target the layers that actually dominate.
//!
//! Run with:
//! `cargo test -p mudu_kernel stmt_path_bench -- --ignored --nocapture`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::protocol::{encode_server_response, ServerResponse};
use mudu_sys::env_var::temp_dir;
use mudu_sys::time::instant_now;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::StmtType;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::index::index_key::key_tuple::KeyTuple;
use crate::mudu_conn::mudu_conn_core::{query_exec_to_rows, MuduConnCore};
use crate::server::test_meta_mgr::TestMetaMgr;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::server::x_contract::utils::build_key_tuple;
use crate::server::x_contract::{WorkerXContract, WorkerXContractParams};
use crate::sql::binder::Binder;
use crate::sql::bound_stmt::BoundStmt;
use crate::sql::plan_ctx::PlanCtx;
use crate::sql::planner::Planner;
use crate::storage::relation::relation::Relation;
use crate::x_engine::api::{OptInsert, OptRead, VecDatum, VecSelTerm, XContract};

const ROWS: i32 = 4000;
const ROWS_PER_COMMIT: i32 = 100;
const ITERS: usize = 1000;
const WARMUP: usize = 100;

fn users_schema() -> SchemaTable {
    use mudu_type::data_type_info::DataTypeInfo;
    let col = |name: &str, family: TypeFamily| {
        SchemaColumn::new(
            name.to_string(),
            family,
            DataTypeInfo::from_opt_object(&DataType::default_for(family)),
        )
    };
    SchemaTable::new(
        "users".to_string(),
        vec![
            col("user_id", TypeFamily::I32),
            col("name", TypeFamily::String),
            col("phone", TypeFamily::String),
            col("email", TypeFamily::String),
            col("password", TypeFamily::String),
            col("created_at", TypeFamily::I32),
            col("updated_at", TypeFamily::I32),
        ],
        vec![0],
        vec![1, 2, 3, 4, 5, 6],
    )
}

fn datum_i32(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn datum_string(v: &str) -> Vec<u8> {
    mudu_type::data_type_function::send_binary(
        &DataValue::from_string(v.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .unwrap()
}

fn key_row(v: i32) -> VecDatum {
    VecDatum::new(vec![(0, datum_i32(v))])
}

fn value_row(v: i32) -> VecDatum {
    VecDatum::new(vec![
        (1, datum_string("alice")),
        (2, datum_string("12345678")),
        (3, datum_string("alice@example.com")),
        (4, datum_string("secret")),
        (5, datum_i32(v)),
        (6, datum_i32(v)),
    ])
}

struct BenchEnv {
    core: Arc<MuduConnCore>,
    contract: Arc<WorkerXContract>,
    meta: Arc<TestMetaMgr>,
    table_id: OID,
    desc: Arc<TableDesc>,
    relation: Arc<Relation>,
    hot_key: i32,
}

async fn setup() -> RS<BenchEnv> {
    let meta = Arc::new(TestMetaMgr::new());
    let schema = users_schema();
    let table_id = schema.id();
    let data_dir = temp_dir()
        .join(format!("stmt_path_bench_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    let worker_id = gen_oid();
    let contract = Arc::new(WorkerXContract::with_log_and_data_dir(
        WorkerXContractParams {
            meta_mgr: meta.clone(),
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id,
            default_unpartitioned_worker_id: worker_id,
            partition_id: 0,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
        },
    )?);
    let ddl = contract.begin_tx().await?;
    contract.create_table(ddl.clone(), &schema).await?;
    contract.commit_tx(ddl).await?;

    let mut key = 1;
    while key <= ROWS {
        let tx = contract.begin_tx().await?;
        for k in key..(key + ROWS_PER_COMMIT).min(ROWS + 1) {
            contract
                .insert(
                    tx.clone(),
                    table_id,
                    &key_row(k),
                    &value_row(k),
                    &OptInsert::default(),
                )
                .await?;
        }
        contract.commit_tx(tx).await?;
        key += ROWS_PER_COMMIT;
    }

    let core = Arc::new(MuduConnCore::new(meta.clone(), None, false)?);
    let desc = meta.get_table_by_id(table_id).await?;
    let relation = contract
        .storage()
        .get_relation_async(table_id, None)
        .await?;
    Ok(BenchEnv {
        core,
        contract,
        meta,
        table_id,
        desc,
        relation,
        hot_key: ROWS,
    })
}

/// Average nanoseconds per iteration of an async operation, after a warmup.
async fn bench_async<F, Fut>(iters: usize, mut f: F) -> u128
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    for _ in 0..WARMUP {
        f().await;
    }
    let start = instant_now();
    for _ in 0..iters {
        f().await;
    }
    start.elapsed().as_nanos() / iters as u128
}

fn report(name: &str, avg_ns: u128) {
    println!("BENCH {name:<28} avg = {:>8} ns", avg_ns);
}

#[test]
#[ignore = "micro-benchmark; run on demand with --ignored --nocapture"]
fn stmt_point_select_breakdown() {
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(async move {
            let r = bench_body().await;
            r.unwrap();
        });
}

async fn bench_body() -> RS<()> {
    let env = setup().await?;
    let sql = format!(
        "SELECT user_id, name, phone, email, password, created_at, updated_at \
         FROM users WHERE user_id = {}",
        env.hot_key
    );
    let x_contract: Arc<dyn XContract> = env.contract.clone();
    let tx = env.contract.begin_tx().await?;

    // Warm the parse cache and verify the statement returns exactly one row.
    let stmt = env.core.parse_one(&sql.as_str())?;
    let (rows, _desc) = env
        .core
        .query_rows(&stmt, Box::new(()), tx.clone(), x_contract.clone())
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values().len(), 7);

    // --- parse layer -------------------------------------------------------
    report(
        "parser_new (SQLParser::new)",
        bench_async(ITERS, || async {
            let _ = SQLParser::new().unwrap();
        })
        .await,
    );
    {
        let core = env.core.clone();
        let sql = sql.clone();
        report(
            "parse_one (cache hit)",
            bench_async(ITERS, move || {
                let core = core.clone();
                let sql = sql.clone();
                async move {
                    let _ = core.parse_one(&sql.as_str()).unwrap();
                }
            })
            .await,
        );
    }
    {
        let stmt = stmt.clone();
        report(
            "stmt_clone (AST deep copy)",
            bench_async(ITERS, move || {
                let stmt = stmt.clone();
                async move {
                    let _: StmtType = (*stmt).clone();
                }
            })
            .await,
        );
    }
    {
        let core = env.core.clone();
        report(
            "parse_one (cache miss)",
            bench_async(200, move || {
                let core = core.clone();
                async move {
                    let unique = format!(
                        "SELECT user_id FROM users WHERE user_id = 1 /* {} */",
                        gen_oid()
                    );
                    let _ = core.parse_one(&unique.as_str()).unwrap();
                }
            })
            .await,
        );
    }

    // --- bind / plan / exec cumulative stages -------------------------------
    let bind_ns = {
        let meta = env.meta.clone();
        let stmt = stmt.clone();
        bench_async(ITERS, move || {
            let meta = meta.clone();
            let stmt = stmt.clone();
            async move {
                let _ = Binder::new(meta).bind_ref(&stmt, &()).await.unwrap();
            }
        })
        .await
    };
    report("bind", bind_ns);

    let bind_plan_ns = {
        let meta = env.meta.clone();
        let stmt = stmt.clone();
        let tx = tx.clone();
        let x_contract = x_contract.clone();
        bench_async(ITERS, move || {
            let meta = meta.clone();
            let stmt = stmt.clone();
            let tx = tx.clone();
            let x_contract = x_contract.clone();
            async move {
                let bound = Binder::new(meta.clone())
                    .bind_ref(&stmt, &())
                    .await
                    .unwrap();
                let BoundStmt::Query(query) = bound else {
                    panic!("expected query")
                };
                let planner = Planner::new(PlanCtx {
                    tx_mgr: tx,
                    meta_mgr: meta,
                    x_contract,
                    async_runtime: None,
                });
                let _ = planner.plan_query(query).await.unwrap();
            }
        })
        .await
    };
    report("bind+plan", bind_plan_ns);
    report("=> plan only", bind_plan_ns.saturating_sub(bind_ns));

    let bind_plan_exec_ns = {
        let meta = env.meta.clone();
        let stmt = stmt.clone();
        let tx = tx.clone();
        let x_contract = x_contract.clone();
        bench_async(ITERS, move || {
            let meta = meta.clone();
            let stmt = stmt.clone();
            let tx = tx.clone();
            let x_contract = x_contract.clone();
            async move {
                let bound = Binder::new(meta.clone())
                    .bind_ref(&stmt, &())
                    .await
                    .unwrap();
                let BoundStmt::Query(query) = bound else {
                    panic!("expected query")
                };
                let planner = Planner::new(PlanCtx {
                    tx_mgr: tx,
                    meta_mgr: meta,
                    x_contract,
                    async_runtime: None,
                });
                let exec = planner.plan_query(query).await.unwrap();
                let (rows, _) = query_exec_to_rows(exec).await.unwrap();
                assert_eq!(rows.len(), 1);
            }
        })
        .await
    };
    report("bind+plan+exec+decode", bind_plan_exec_ns);
    report(
        "=> exec+decode only",
        bind_plan_exec_ns.saturating_sub(bind_plan_ns),
    );

    // --- full statement ------------------------------------------------------
    {
        let core = env.core.clone();
        let sql = sql.clone();
        let tx = tx.clone();
        let x_contract = x_contract.clone();
        report(
            "query total (parse+bind+plan+exec)",
            bench_async(ITERS, move || {
                let core = core.clone();
                let sql = sql.clone();
                let tx = tx.clone();
                let x_contract = x_contract.clone();
                async move {
                    let stmt = core.parse_one(&sql.as_str()).unwrap();
                    let (rows, _) = core
                        .query_rows(&stmt, Box::new(()), tx, x_contract)
                        .await
                        .unwrap();
                    assert_eq!(rows.len(), 1);
                }
            })
            .await,
        );
    }

    // --- storage read path ---------------------------------------------------
    {
        let contract = env.contract.clone();
        let table_id = env.table_id;
        let hot_key = env.hot_key;
        let tx = tx.clone();
        report(
            "read_key direct (storage+project)",
            bench_async(ITERS, move || {
                let contract = contract.clone();
                let tx = tx.clone();
                async move {
                    let row = contract
                        .read_key(
                            tx,
                            table_id,
                            &key_row(hot_key),
                            &VecSelTerm::new(vec![0, 1, 2, 3, 4, 5, 6]),
                            &OptRead::default(),
                        )
                        .await
                        .unwrap();
                    assert!(row.is_some());
                }
            })
            .await,
        );
    }

    // index.get + version chain vs + value_file payload read.
    let key_tuple = KeyTuple::from(build_key_tuple(&key_row(env.hot_key), &env.desc)?);
    let snapshot = WorkerSnapshot::new(u64::MAX / 2, vec![]);
    let meta_ns = {
        let relation = env.relation.clone();
        let key_tuple = key_tuple.clone();
        let snapshot = snapshot.clone();
        bench_async(ITERS, move || {
            let relation = relation.clone();
            let key_tuple = key_tuple.clone();
            let snapshot = snapshot.clone();
            async move {
                assert!(relation
                    .has_visible_version(&key_tuple, &snapshot)
                    .await
                    .unwrap());
            }
        })
        .await
    };
    report("relation: index.get+version", meta_ns);
    let value_ns = {
        let relation = env.relation.clone();
        let key_tuple = key_tuple.clone();
        let snapshot = snapshot.clone();
        bench_async(ITERS, move || {
            let relation = relation.clone();
            let key_tuple = key_tuple.clone();
            let snapshot = snapshot.clone();
            async move {
                assert!(relation
                    .visible_value(&key_tuple, &snapshot)
                    .await
                    .unwrap()
                    .is_some());
            }
        })
        .await
    };
    report("relation: visible_value", value_ns);
    report("=> value_file.get+copy", value_ns.saturating_sub(meta_ns));

    // storage.get_on_partition: tx overlay check + relation lookup + visible read.
    {
        let storage = env.contract.storage().clone();
        let table_id = env.table_id;
        let tx = tx.clone();
        let key_bytes = build_key_tuple(&key_row(env.hot_key), &env.desc)?;
        report(
            "storage.get_on_partition",
            bench_async(ITERS, move || {
                let storage = storage.clone();
                let tx = tx.clone();
                let key_bytes = key_bytes.clone();
                async move {
                    assert!(storage
                        .get_on_partition(table_id, None, &key_bytes, tx.as_ref())
                        .await
                        .unwrap()
                        .is_some());
                }
            })
            .await,
        );
    }

    // project_selected_fields on pre-fetched key/value bytes.
    {
        let key_bytes = build_key_tuple(&key_row(env.hot_key), &env.desc)?;
        let value_bytes = env
            .relation
            .visible_value(&key_tuple, &snapshot)
            .await?
            .unwrap();
        let desc = env.desc.clone();
        let (kb, vb, d1) = (key_bytes.clone(), value_bytes.clone(), desc.clone());
        report(
            "project_selected_fields (7 cols)",
            bench_async(ITERS, move || {
                let kb = kb.clone();
                let vb = vb.clone();
                let d1 = d1.clone();
                async move {
                    let row = crate::server::x_contract::utils::project_selected_fields(
                        &d1,
                        &kb,
                        &vb,
                        &VecSelTerm::new(vec![0, 1, 2, 3, 4, 5, 6]),
                    )
                    .unwrap();
                    assert_eq!(row.len(), 7);
                }
            })
            .await,
        );

        // TypedBin decode of the projected row, as tuple_field_to_value does.
        let row = crate::server::x_contract::utils::project_selected_fields(
            &desc,
            &key_bytes,
            &value_bytes,
            &VecSelTerm::new(vec![0, 1, 2, 3, 4, 5, 6]),
        )?;
        let tuple_desc =
            crate::executor::project_tuple_desc(&desc, &VecSelTerm::new(vec![0, 1, 2, 3, 4, 5, 6]));
        report(
            "typed_bin decode (7 fields)",
            bench_async(ITERS, move || {
                let row = row.clone();
                let tuple_desc = tuple_desc.clone();
                async move {
                    let mut values = Vec::with_capacity(row.len());
                    for (index, field) in row.iter().enumerate() {
                        let datum_desc = &tuple_desc.fields()[index];
                        let typed = mudu_contract::tuple::typed_bin::TypedBin::new(
                            datum_desc.type_family(),
                            field.clone().unwrap(),
                        );
                        values.push(typed.to_value(datum_desc.data_type()).unwrap());
                    }
                    assert_eq!(values.len(), 7);
                }
            })
            .await,
        );
    }

    // Standalone BTreeIndex.get with the same comparator/desc and row count,
    // isolating index lookup from the version-chain read.
    {
        use crate::index::btree::btree_index::BTreeIndex;
        use crate::index::index_key::compare_context::CompareContext;
        use mudu_contract::tuple::comparator::TupleComparator;

        let index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: TupleComparator::new(),
            desc: env.desc.key_desc().clone(),
        });
        for k in 1..=ROWS {
            index
                .insert(
                    KeyTuple::from(build_key_tuple(&key_row(k), &env.desc)?),
                    k as u64,
                )
                .unwrap();
        }
        let hot = KeyTuple::from(build_key_tuple(&key_row(env.hot_key), &env.desc)?);
        // index.get is synchronous; time it with a plain loop.
        for _ in 0..WARMUP {
            assert!(index.get(&hot).unwrap().is_some());
        }
        let start = instant_now();
        for _ in 0..ITERS {
            assert!(index.get(&hot).unwrap().is_some());
        }
        report(
            "btree index.get only",
            start.elapsed().as_nanos() / ITERS as u128,
        );

        // len() performs the same per-call context setup and read lock but no
        // key comparisons, isolating the fixed overhead of a BTreeIndex call.
        let start = instant_now();
        for _ in 0..ITERS {
            assert_eq!(index.len().unwrap(), ROWS as usize);
        }
        report(
            "btree len (ctx setup only)",
            start.elapsed().as_nanos() / ITERS as u128,
        );

        // Same map with a raw memcmp comparator, isolating the datum-decoding
        // comparator cost of the real index.
        fn memcmp_compare(
            left: &[u8],
            right: &[u8],
            _desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
        ) -> RS<std::cmp::Ordering> {
            Ok(left.cmp(right))
        }
        fn memcmp_equal(
            left: &[u8],
            right: &[u8],
            _desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
        ) -> RS<bool> {
            Ok(left == right)
        }
        fn memcmp_hash_one(
            tuple: &[u8],
            _desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
            hasher: &mut dyn std::hash::Hasher,
        ) -> RS<()> {
            hasher.write(tuple);
            Ok(())
        }
        fn memcmp_hash_finish(
            tuple: &[u8],
            desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
            hasher: &mut dyn std::hash::Hasher,
        ) -> RS<u64> {
            memcmp_hash_one(tuple, desc, hasher)?;
            Ok(hasher.finish())
        }
        let raw_index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: TupleComparator {
                compare: memcmp_compare,
                equal: memcmp_equal,
                hash_cal_one: memcmp_hash_one,
                hash_cal_finish: memcmp_hash_finish,
            },
            desc: env.desc.key_desc().clone(),
        });
        for k in 1..=ROWS {
            raw_index
                .insert(
                    KeyTuple::from(build_key_tuple(&key_row(k), &env.desc)?),
                    k as u64,
                )
                .unwrap();
        }
        for _ in 0..WARMUP {
            assert!(raw_index.get(&hot).unwrap().is_some());
        }
        let start = instant_now();
        for _ in 0..ITERS {
            assert!(raw_index.get(&hot).unwrap().is_some());
        }
        report(
            "btree get (memcmp comparator)",
            start.elapsed().as_nanos() / ITERS as u128,
        );
    }

    // --- response path -------------------------------------------------------
    let stmt = env.core.parse_one(&sql.as_str())?;
    let (rows, desc) = env
        .core
        .query_rows(&stmt, Box::new(()), tx.clone(), x_contract.clone())
        .await?;
    report(
        "encode_server_response (rmp+frame)",
        bench_async(ITERS, move || {
            let rows = rows.clone();
            let desc = desc.clone();
            async move {
                let response = ServerResponse::new(desc.clone(), rows.clone(), 0, None);
                let bytes = encode_server_response(1, &response).unwrap();
                assert!(!bytes.is_empty());
            }
        })
        .await,
    );

    // --- UPDATE statement ------------------------------------------------------
    let update_sql = format!(
        "UPDATE users SET name = 'bob', updated_at = 7 WHERE user_id = {}",
        env.hot_key
    );
    let update_stmt = env.core.parse_one(&update_sql.as_str())?;
    {
        let core = env.core.clone();
        let tx = tx.clone();
        let x_contract = x_contract.clone();
        report(
            "update total (parse+bind+plan+run)",
            bench_async(ITERS, move || {
                let core = core.clone();
                let tx = tx.clone();
                let x_contract = x_contract.clone();
                let update_stmt = update_stmt.clone();
                async move {
                    let n = core
                        .execute(&update_stmt, Box::new(()), tx, x_contract)
                        .await
                        .unwrap();
                    assert_eq!(n, 1);
                }
            })
            .await,
        );
    }

    env.contract.abort_tx(tx).await?;
    Ok(())
}
