#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn valid_checksums_do_not_bypass_structure_validation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir().join(format!(
            "overlay-validation-{}",
            mudu_sys::random::uuid_v4()
        ));
        let fs = mudu_sys::fs::tokio_fs();
        repl::durable_io::ensure_dir(fs.as_ref(), &dir)
            .await
            .unwrap();
        for mutation in 0..4 {
            let op = MetaOp::AbortMigration {
                rule: "r".into(),
                partition: "p".into(),
            };
            let mut image = Image {
                format: 1,
                cluster_id: 9,
                cursor: OverlayCursor {
                    version: 3,
                    payload: rmp_serde::to_vec(&op).unwrap(),
                },
                entries: vec![(
                    "r".into(),
                    "q".into(),
                    PlacementOverride::Moved {
                        group_id: 1,
                        worker_index: 0,
                    },
                    2,
                )],
            };
            match mutation {
                0 => image.format = 0,
                1 => image.entries.push(image.entries[0].clone()),
                2 => image.entries[0].3 = 4,
                _ => {
                    image.cursor.payload = rmp_serde::to_vec(&MetaOp::DropTable { oid: 1 }).unwrap()
                }
            }
            let payload = rmp_serde::to_vec(&image).unwrap();
            let bytes = rmp_serde::to_vec(&Envelope {
                crc: mudu::common::crc::crc64(&payload),
                payload,
            })
            .unwrap();
            repl::durable_io::publish(fs.as_ref(), &dir.join(FILE), &bytes)
                .await
                .unwrap();
            assert!(load_overlay(&dir, 9).is_err());
        }
        // An unreadable path type must never be interpreted as a missing image.
        let file = dir.join("not-a-directory");
        repl::durable_io::publish(fs.as_ref(), &file, b"file")
            .await
            .unwrap();
        assert!(load_overlay(&file, 9).is_err());
    })
    .unwrap();
}
