use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_backtrace::Location as BtLoc;
use lazy_static::lazy_static;
use scc::HashIndex;

use crate::notifier::Notifier;
use mudu::common::result::RS;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::{select, task, task_local};
use tracing::trace;
use uuid::Uuid;

/// TaskID use to store async task related context
/// Any async function can have a TaskID parameter to retrieve this task context
/// If rust can support [Custom Future contexts](https://github.com/rust-lang/rfcs/issues/2900)
/// The context information can be kept in Future
pub type TaskID = u128;

task_local! {
    static TASK_ID: TaskID;
}

pub struct TaskContext {
    name: String,
    notifier: Notifier,
    local_task: bool,
    id: u128,
    backtrace: Mutex<VecDeque<BtLoc>>,
}

pub struct Trace {}

impl Trace {
    pub fn new(location: BtLoc) -> Self {
        Self::enter(location);
        Self {}
    }

    fn enter(location: BtLoc) {
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.enter(location);
        }
    }

    fn exit() {
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        if let Some(_t) = opt {
            _t.exit();
        }
    }

    pub fn backtrace() -> String {
        let _id = this_task_id();
        let opt = TaskContext::get(_id);
        match opt {
            Some(_t) => _t.backtrace(),
            _ => "".to_string(),
        }
    }

    pub fn dump_task_trace() -> String {
        let mut ret = String::new();
        let guard = scc::Guard::new();
        for (_id, task) in TASK_CONTEXT.iter(&guard) {
            let s = format!(
                "name:{},\t id: {},\t trace {}\n",
                task.name(),
                _id,
                task.backtrace()
            );
            ret.push_str(s.as_str());
        }
        ret
    }
}

impl Drop for Trace {
    fn drop(&mut self) {
        Trace::exit()
    }
}

#[macro_export]
macro_rules! task_trace {
    () => {{
        let s = async_backtrace::location!();
        $crate::task::Trace::new(s)
    }};
}

#[macro_export]
macro_rules! dump_task_trace {
    () => {{
        $crate::task::Trace::dump_task_trace()
    }};
}

#[macro_export]
macro_rules! task_backtrace {
    () => {{
        $crate::task::Trace::backtrace()
    }};
}

#[macro_export]
macro_rules! this_task_id {
    () => {{
        $crate::task::this_task_id()
    }};
}
/// The task must create by `task::spawn_local_task`, or `task::spawn_task` to set `TASK_ID` value.
/// if not, the `LocalKey::get` would raise such panic,
///     "cannot access a task-local storage value without setting it first"
pub fn this_task_id() -> TaskID {
    TASK_ID.get()
}

fn new_task_id() -> TaskID {
    Uuid::new_v4().as_u128()
}

impl TaskContext {
    fn new_context(id: TaskID, name: String, local_task: bool, notifier: Notifier) -> Arc<Self> {
        let r = Self {
            name,
            notifier,
            local_task,
            id,
            backtrace: Default::default(),
        };
        let ret = Arc::new(r);
        let id = ret.id();
        let _ = TASK_CONTEXT.insert_sync(id, ret.clone());
        ret
    }

    fn remove_context(id: TaskID) {
        let _ = TASK_CONTEXT.remove_sync(&id);
    }

    pub fn get(id: TaskID) -> Option<Arc<TaskContext>> {
        let opt = TASK_CONTEXT.get_sync(&id);
        opt.map(|e| e.get().clone())
    }

    pub fn is_local(&self) -> bool {
        self.local_task
    }

    pub fn id(&self) -> TaskID {
        self.id
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn notifier(&self) -> Notifier {
        self.notifier.clone()
    }

    pub fn enter(&self, l: BtLoc) {
        let mut location = self.backtrace.lock().unwrap();
        location.push_back(l);
    }

    pub fn exit(&self) {
        let mut location = self.backtrace.lock().unwrap();
        let _ = location.pop_back();
    }

    pub fn backtrace(&self) -> String {
        let deque = self.backtrace.lock().unwrap();
        let mut s = String::new();
        s.push_str("backtrace:\n");
        for (n, l) in deque.iter().enumerate() {
            s.push_str("  ");
            for _ in 0..n {
                s.push_str("--");
            }
            s.push_str("->");
            s.push_str(l.to_string().as_str());
            s.push('\n');
        }
        s
    }
}

lazy_static! {
    static ref TASK_CONTEXT: HashIndex<TaskID, Arc<TaskContext>> = HashIndex::new();
}

pub fn spawn_local_task<F>(
    cancel_notifier: Notifier,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let id = new_task_id();
    let _ = TaskContext::new_context(id, _name.to_string(), false, cancel_notifier.clone());
    Ok(task::spawn_local(TASK_ID.scope(id, async move {
        let r = __select_local_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        r
    })))
}

pub fn spawn_task<F>(
    cancel_notifier: Notifier,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let id = new_task_id();
    let _ = TaskContext::new_context(id, _name.to_string(), false, cancel_notifier.clone());
    Ok(task::spawn(TASK_ID.scope(id, async move {
        let r = __select_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        r
    })))
}

pub fn spawn_local_task_timeout<F>(
    cancel_notifier: Notifier,
    duration: Duration,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Result<F::Output, TaskFailed>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Ok(task::spawn_local(async move {
        __select_local_till_done_or_timeout(cancel_notifier, duration, future).await
    }))
}

async fn __select_local_till_done<F>(notify: Notifier, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("local task stop");
                None
            }
            r = future => {
                trace ! ("local task  end");
                Some(r)
            }
        };
        r
    };
    future.await
}

pub enum TaskFailed {
    Cancel,
    Timeout,
}

async fn __select_local_till_done_or_timeout<F>(
    notify: Notifier,
    duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("local task stop");
                 Err(TaskFailed::Cancel)
            }
            r = future => {
                trace ! ("local task  end");
                Ok(r)
            }
            _ = sleep(duration) => {
                Err(TaskFailed::Timeout)
            }
        };
        r
    };
    future.await
}

async fn __select_till_done<F>(notify: Notifier, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: Send + 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("task stop");
                None
            }
            r = future => {
                trace ! ("task  end");
                Some(r)
            }
        };
        r
    };
    future.await
}
