use crate::Args;
use mudu_client::management::{
    ServerTopology, fetch_server_topology, is_server_topology_unsupported,
};
use mudu_sys::task::sync::spawn_thread_named;
use mudu_sys::tokio::runtime::Builder;
use mududb::common::result::RS;
use mududb::error::ErrorCode::{InvalidState, Network, Thread, Tokio};
use mududb::mudu_error;

pub(crate) fn load_sync_topology() -> RS<ServerTopology> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Err(mudu_error!(
            InvalidState,
            "warehouse-partitioned benchmark requires a mudud connection with http_addr"
        ));
    };
    // Fetch on a fresh thread: building a runtime is only legal on a thread
    // that does not already carry one.
    let handle = spawn_thread_named("tpcc-topology-fetch".to_string(), move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| mudu_error!(Tokio, "build tpcc topology runtime error", e))?;
        match runtime.block_on(fetch_server_topology(&http_addr)) {
            Ok(topology) => Ok(topology),
            Err(err) if is_server_topology_unsupported(&err) => Err(mudu_error!(
                InvalidState,
                "warehouse-partitioned benchmark requires server topology support"
            )),
            Err(err) => Err(mudu_error!(Network, err)),
        }
    })?;
    let topology = handle
        .join()
        .map_err(|_| mudu_error!(Thread, "join topology fetch thread error"))??;
    Ok(topology)
}

pub(crate) async fn load_async_topology(http_addr: &str) -> RS<ServerTopology> {
    match fetch_server_topology(http_addr).await {
        Ok(topology) => Ok(topology),
        Err(err) if is_server_topology_unsupported(&err) => Err(mudu_error!(
            InvalidState,
            "warehouse-partitioned benchmark requires server topology support"
        )),
        Err(err) => Err(mudu_error!(Network, err)),
    }
}

/// Upper bound on range partitions per rule: the whole rule serializes into
/// one catalog row that must fit a 4 KiB storage page.
pub(crate) const MAX_PARTITION_COUNT: usize = 50;

/// Effective number of range partitions for warehouse-partitioned runs:
/// `--partition-count` when given, otherwise one per worker; always capped by
/// the warehouse count.
pub(crate) fn effective_partition_count(args: &Args, worker_count: usize) -> usize {
    args.partition_count
        .unwrap_or(worker_count)
        .max(1)
        .min(args.warehouses.max(1) as usize)
}

/// Computes the warehouse id ranges covered by each partition.
///
/// The partition count defaults to the server worker count (capped by the
/// warehouse count) and can be raised with `--partition-count`: a rule with
/// too many partitions serializes into a single catalog row that exceeds the
/// 4 KiB storage page (see `MAX_PARTITION_COUNT`). Each partition covers a
/// contiguous warehouse id range `[start, end)`.
pub(crate) fn partition_ranges(warehouses: i32, worker_count: usize) -> Vec<(i32, i32)> {
    let count = worker_count.max(1).min(warehouses.max(1) as usize);
    let base = warehouses / count as i32;
    let remainder = warehouses % count as i32;
    let mut start = 1;
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        let size = base + i32::from((index as i32) < remainder);
        ranges.push((start, start + size));
        start += size;
    }
    ranges
}

/// Maps a warehouse to the index of the partition that owns it. Partition
/// `i` is placed on worker `i % worker_count` (see
/// `build_partition_placement_sql`).
pub(crate) fn partition_index_for_warehouse(warehouse_id: i32, ranges: &[(i32, i32)]) -> usize {
    ranges
        .iter()
        .position(|(start, end)| warehouse_id >= *start && warehouse_id < *end)
        .unwrap_or(ranges.len() - 1)
}

pub(crate) fn build_partition_rule_sql(args: &Args, worker_count: usize) -> String {
    let partitions = partition_ranges(
        args.warehouses,
        effective_partition_count(args, worker_count),
    )
    .iter()
    .enumerate()
    .map(|(index, (start, end))| {
        format!("PARTITION p{} VALUES FROM ({start}) TO ({end})", index + 1)
    })
    .collect::<Vec<_>>()
    .join(", ");
    format!("CREATE PARTITION RULE r_tpcc_wh RANGE ({partitions})")
}

pub(crate) fn build_partition_placement_sql(args: &Args, topology: &ServerTopology) -> RS<String> {
    if topology.workers.is_empty() {
        return Err(mudu_error!(
            InvalidState,
            "server topology exposes no workers"
        ));
    }
    let partition_count = partition_ranges(
        args.warehouses,
        effective_partition_count(args, topology.workers.len()),
    )
    .len();
    let placements = (1..=partition_count)
        .map(|index| {
            let worker = &topology.workers[(index - 1) % topology.workers.len()];
            format!("PARTITION p{index} ON WORKER {}", worker.worker_id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "CREATE PARTITION PLACEMENT FOR RULE r_tpcc_wh ({placements})"
    ))
}

#[cfg(test)]
mod partition_plan_tests {
    use super::{build_partition_rule_sql, partition_index_for_warehouse, partition_ranges};
    use crate::Args;
    use crate::setup::warehouse_shard_ranges;
    use clap::Parser;

    #[test]
    fn warehouse_shard_ranges_cover_all_warehouses_contiguously() {
        let ranges = warehouse_shard_ranges(400, 16);
        assert_eq!(ranges.len(), 16);
        assert_eq!(ranges[0], (1, 25));
        assert_eq!(ranges[15], (376, 400));
        for (index, (start, end)) in ranges.iter().enumerate() {
            assert!(end >= start);
            if index > 0 {
                assert_eq!(*start, ranges[index - 1].1 + 1);
            }
        }

        // Shard count is capped by the warehouse count.
        assert_eq!(warehouse_shard_ranges(2, 16), vec![(1, 1), (2, 2)]);
        // Remainder warehouses go to the leading shards.
        assert_eq!(warehouse_shard_ranges(5, 2), vec![(1, 3), (4, 5)]);
    }

    #[test]
    fn partition_ranges_split_warehouses_evenly_across_workers() {
        let ranges = partition_ranges(400, 8);
        assert_eq!(ranges.len(), 8);
        assert_eq!(ranges[0], (1, 51));
        assert_eq!(ranges[7], (351, 401));
        for (index, (start, end)) in ranges.iter().enumerate() {
            assert_eq!(end - start, 50);
            if index > 0 {
                assert_eq!(*start, ranges[index - 1].1);
            }
        }
    }

    #[test]
    fn partition_ranges_distribute_remainder_and_cap_at_warehouse_count() {
        let ranges = partition_ranges(10, 8);
        assert_eq!(
            ranges,
            vec![
                (1, 3),
                (3, 5),
                (5, 6),
                (6, 7),
                (7, 8),
                (8, 9),
                (9, 10),
                (10, 11)
            ]
        );

        let capped = partition_ranges(2, 8);
        assert_eq!(capped, vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn partition_index_for_warehouse_matches_range_bounds() {
        let ranges = partition_ranges(400, 8);
        assert_eq!(partition_index_for_warehouse(1, &ranges), 0);
        assert_eq!(partition_index_for_warehouse(50, &ranges), 0);
        assert_eq!(partition_index_for_warehouse(51, &ranges), 1);
        assert_eq!(partition_index_for_warehouse(400, &ranges), 7);
    }

    #[test]
    fn effective_partition_count_honors_arg_and_caps() {
        let default_args = Args::parse_from(["tpcc-benchmark", "--warehouses", "400"]);
        assert_eq!(super::effective_partition_count(&default_args, 8), 8);

        let custom_args = Args::parse_from([
            "tpcc-benchmark",
            "--warehouses",
            "400",
            "--partition-count",
            "32",
        ]);
        assert_eq!(super::effective_partition_count(&custom_args, 8), 32);

        // Capped by the warehouse count.
        let tiny_args = Args::parse_from([
            "tpcc-benchmark",
            "--warehouses",
            "2",
            "--partition-count",
            "32",
        ]);
        assert_eq!(super::effective_partition_count(&tiny_args, 8), 2);
    }

    #[test]
    fn build_partition_rule_sql_creates_one_partition_per_worker() {
        let args = Args::parse_from(["tpcc-benchmark", "--warehouses", "400"]);
        let sql = build_partition_rule_sql(&args, 8);
        assert_eq!(
            sql,
            "CREATE PARTITION RULE r_tpcc_wh RANGE (\
            PARTITION p1 VALUES FROM (1) TO (51), \
            PARTITION p2 VALUES FROM (51) TO (101), \
            PARTITION p3 VALUES FROM (101) TO (151), \
            PARTITION p4 VALUES FROM (151) TO (201), \
            PARTITION p5 VALUES FROM (201) TO (251), \
            PARTITION p6 VALUES FROM (251) TO (301), \
            PARTITION p7 VALUES FROM (301) TO (351), \
            PARTITION p8 VALUES FROM (351) TO (401))"
        );
    }
}
