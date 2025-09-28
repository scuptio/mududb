#[cfg(test)]
mod _test {
    use std::thread;

    use crate::contract::pst_op_list::PstOpList;
    use crate::contract::timestamp::Timestamp;
    use crate::storage::pst_store_factory::PstStoreFactory;
    use mudu::common::result::RS;
    use mudu_utils::notifier::Notifier;

    use mudu_utils::log::log_setup;
    use tokio::sync::oneshot;
    use tracing::{error, info};

    #[test]
    fn test_pst_store() {
        log_setup("debug");
        _test_pst_store().unwrap();
        info!("test_pst_store test success");
    }

    fn _test_pst_store() -> RS<()> {
        let db = format!("/tmp/test_pst_store_{}", uuid::Uuid::new_v4());
        let (task, ch) = PstStoreFactory::create(db).unwrap();
        let thd_task = thread::Builder::new().spawn(move || {
            let r = task.run_once();
            match r {
                Ok(_) => {}
                Err(e) => {
                    error!("run flush task error {}", e);
                    panic!("{}", e);
                }
            }
        });
        let post_task = thread::Builder::new().spawn(move || {
            for i in 0..1000 {
                let mut ops = PstOpList::new();
                ops.push_insert(
                    i,
                    i,
                    Timestamp::new(0, 1),
                    Default::default(),
                    Default::default(),
                );
                ops.push_update(i, i, Timestamp::new(2, 3), Default::default());

                ch.async_run(ops).unwrap();
                let notifier = Notifier::new();
                let mut ops = PstOpList::new();
                ops.push_delete(i, i);
                ch.async_run(ops).unwrap();
            }
            let (s, r) = oneshot::channel();
            let notifier = Notifier::new();
            let mut ops = PstOpList::new();
            ops.push_stop(s);
            ch.async_run(ops).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                r.await.unwrap();
                info!("notified");
            });
        });

        post_task.unwrap().join().unwrap();
        thd_task.unwrap().join().unwrap();
        Ok(())
    }
}
