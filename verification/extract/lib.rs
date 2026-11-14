//! Extraction root: uses the production module verbatim, without feature overrides.
#[path = "../../crates/common/mudu/src/utils/buf.rs"]
pub mod buf;

#[path = "../../crates/db-kernel/storage/src/page/record_slot_core.rs"]
pub mod record_slot_core;
#[path = "../../crates/db-kernel/storage/src/page/layout_math.rs"]
pub mod layout_math;
#[path = "../../crates/db-kernel/wal/src/worker_wal_backend/layout/decision.rs"]
pub mod decision;

#[path = "../../crates/db-kernel/wal/src/worker_wal_backend/append_plan.rs"]
pub mod append_plan;
#[path = "../../crates/db-kernel/wal/src/frame_bounds.rs"]
pub mod frame_bounds;

#[path = "../../crates/db-kernel/wal/src/part_chain.rs"]
pub mod part_chain;

#[path = "../../crates/db-kernel/wal/src/worker_wal_backend/layout/truncate_plan.rs"]
pub mod truncate_plan;

#[path = "../../crates/db-kernel/storage/src/page/sequence_edit.rs"]
pub mod sequence_edit;

#[path = "../../crates/db-kernel/storage/src/time_series/time_series_file/delta_value.rs"]
pub mod delta_value;

#[path = "../../crates/common/mudu_binding/src/universal/mp_core.rs"]
pub mod mp_core;

#[path = "../../crates/common/mudu/src/utils/mssp_frame.rs"]
pub mod mssp_frame;

#[path = "../../crates/db-kernel/mudu_raft/src/core/quorum_core.rs"]
pub mod quorum_core;

#[path = "../../crates/db-kernel/mudu_raft/src/core/vote_core.rs"]
pub mod vote_core;
