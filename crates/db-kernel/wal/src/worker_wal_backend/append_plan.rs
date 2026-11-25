//! Pure append reservations. The caller publishes the returned state only on success.

/// Logical append cursor, independent of paths and open file handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppendState {
    pub current: Option<u64>,
    pub size: u64,
    pub next: u64,
}

/// One reserved interval and the cursor after reserving it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppendPlan {
    pub state: AppendState,
    pub sequence: u64,
    pub offset: u64,
    pub flush: bool,
}

/// Plans an append without mutating the caller, rejecting identifier exhaustion.
/// A valid input has size zero without a current chunk, and otherwise size at
/// most capacity and a current sequence strictly below next.
pub(crate) fn reserve(state: AppendState, capacity: u64, length: u64) -> Option<AppendPlan> {
    if length == 0 {
        let sequence = match state.current {
            Some(sequence) => sequence,
            None => state.next,
        };
        return Some(AppendPlan {
            state,
            sequence,
            offset: state.size,
            flush: false,
        });
    }
    if length <= capacity {
        if let Some(sequence) = state.current {
            if state.size <= capacity && length <= capacity - state.size {
                let end = state.size + length;
                let next_state = if end == capacity {
                    AppendState {
                        current: None,
                        size: 0,
                        next: state.next,
                    }
                } else {
                    AppendState {
                        current: Some(sequence),
                        size: end,
                        next: state.next,
                    }
                };
                return Some(AppendPlan {
                    state: next_state,
                    sequence,
                    offset: state.size,
                    flush: false,
                });
            }
        }
    }
    if state.next == u64::MAX {
        return None;
    }
    let next = state.next + 1;
    let next_state = if length >= capacity {
        AppendState {
            current: None,
            size: 0,
            next,
        }
    } else {
        AppendState {
            current: Some(state.next),
            size: length,
            next,
        }
    };
    Some(AppendPlan {
        state: next_state,
        sequence: state.next,
        offset: 0,
        flush: length > capacity,
    })
}
