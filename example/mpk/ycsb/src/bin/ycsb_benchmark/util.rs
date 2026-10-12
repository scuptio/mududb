use crate::cli::Op;

pub(crate) fn shard_bounds(total: usize, shards: usize, index: usize) -> (usize, usize) {
    let base = total / shards;
    let rem = total % shards;
    let start = index * base + index.min(rem);
    let len = base + usize::from(index < rem);
    (start, start + len)
}

pub(crate) fn build_value(field_length: usize) -> String {
    let mut value = String::with_capacity(field_length);
    for idx in 0..field_length {
        value.push((b'a' + (idx % 26) as u8) as char);
    }
    value
}

fn user_key(idx: usize) -> String {
    format!("user{:016}", idx)
}

pub(crate) fn partitioned_user_key(partition_id: u128, idx: usize) -> String {
    format!("p{:032x}/{}", partition_id, user_key(idx))
}

pub(crate) fn worker_partition(worker_idx: usize, partition_count: usize) -> usize {
    worker_idx % partition_count.max(1)
}

pub(crate) fn sample_owned_key(owned_keys: &[usize], rng: &mut Lcg) -> usize {
    let idx = rng.next_usize(owned_keys.len().max(1));
    owned_keys[idx]
}

pub(crate) fn choose_op(workload: &str, rng: &mut Lcg) -> Op {
    let p = rng.next_usize(100);
    match workload.to_ascii_lowercase().as_str() {
        "a" => {
            if p < 50 {
                Op::Read
            } else {
                Op::Update
            }
        }
        "b" => {
            if p < 95 {
                Op::Read
            } else {
                Op::Update
            }
        }
        "c" => Op::Read,
        "e" => {
            if p < 95 {
                Op::Scan
            } else {
                Op::Insert
            }
        }
        "f" => {
            if p < 50 {
                Op::Read
            } else {
                Op::ReadModifyWrite
            }
        }
        _ => Op::Read,
    }
}

pub(crate) struct Lcg {
    state: u64,
}

impl Lcg {
    pub(crate) fn new(seed: u64) -> Self {
        let seed = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    pub(crate) fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}
