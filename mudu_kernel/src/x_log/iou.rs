use crate::common::buf::Buf;
use libc::iovec;
use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::Result;
use std::os::fd::RawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::sleep;
use tracing::debug;

pub type IoChRecv<T> = Receiver<T>;
pub type IoChSender<T> = Sender<T>;

const SECTOR_SIZE: u64 = 512;
const QUEUE_SIZE: u64 = 64;

const BUF_COUNT: u64 = 64;
const BUF_SIZE: u64 = 1024 * 1024;

pub struct IOUSetting {
    pub sector_size: u64,
    pub buffer_size: u64,
    pub buffer_count: u64,
    pub queue_size: u64,
}

impl Default for IOUSetting {
    fn default() -> Self {
        Self {
            sector_size: SECTOR_SIZE,
            buffer_size: BUF_SIZE,
            buffer_count: BUF_COUNT,
            queue_size: QUEUE_SIZE,
        }
    }
}
struct IoUring {
    ring: rliburing::io_uring,
    param: rliburing::io_uring_params,
    iovec: Vec<iovec>,
    fds: Vec<RawFd>,
    align_byte: u64,
}

pub async fn io_uring_event_loop<
    E: Send + 'static,
    U: Clone + Debug + Send + 'static,
    F: Fn(E) -> (Buf, U),
    C: Fn(Vec<U>),
>(
    file_path: Vec<String>,
    receiver: IoChRecv<E>,
    to_user_data: F,
    on_completion: C,
    setting: IOUSetting,
) -> Result<()> {
    _io_uring_event_loop(file_path, receiver, to_user_data, on_completion, setting).await
}

async fn iou_event_loop_handle<
    E: Send + 'static,
    U: Clone + Debug + Send + 'static,
    F: Fn(E) -> (Buf, U),
    C: Fn(Vec<U>),
>(
    receiver: IoChRecv<E>,
    iovec: Vec<iovec>,
    fds: Vec<RawFd>,
    to_user_data: F,
    on_completion: C,
    setting: IOUSetting,
) -> Result<()> {
    iou_event_loop_handle_gut(
        receiver,
        iovec,
        fds,
        &to_user_data,
        &on_completion,
        &setting,
    )
        .await
}

struct IoWrite {
    file_index: u32,
    buf_index: u32,
    data_size: u32,
    file_offset: u64,
}

struct IoVecBuf<U: Clone + Debug + Send + 'static> {
    available_buf: usize,
    next_buf_index: usize,
    align_byte: u64,
    buf: Vec<(iovec, Option<Vec<U>>)>,
}

struct FileOffset {
    file_fd: u32,
    file_index: u32,
    offset: u64,
}

fn other_io_error(string: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, string)
}

macro_rules! iou_error {
    ($($arg:tt)*) => {
        other_io_error(format!($($arg)*))
    }
}

fn write_to_iovec_one<U: Clone + Debug + Send + 'static>(
    log_write: &mut Vec<(Buf, U)>,
    file_offset: &mut FileOffset,
    iovec_buf: &mut IoVecBuf<U>,
    io_write: &mut Vec<IoWrite>,
    iovec_index: usize,
    to_write_index: usize,
) -> usize {
    let pair = &mut iovec_buf.buf[iovec_index];
    let n = log_write.len();
    let mut buf_offset = 0;
    let mut write_index = n;
    let mut user_data = vec![];
    for i in to_write_index..n {
        let (buf, u) = &log_write[i];
        if buf.len() > pair.0.iov_len as _ {
            panic!("buffer overflow")
        }
        if buf_offset + buf.len() > pair.0.iov_len as _ {
            write_index = i;
            break;
        }
        unsafe {
            libc::memcpy(
                pair.0.iov_base.add(buf_offset) as _,
                buf.as_ptr() as _,
                buf.len() as _,
            )
        };
        buf_offset += buf.len();
        user_data.push(u.clone());
    }
    let to_write_len = round_up_align(buf_offset as _, iovec_buf.align_byte);
    if to_write_len > pair.0.iov_len as _ {
        panic!("buffer overflow")
    }
    unsafe {
        libc::memset(
            pair.0.iov_base.add(buf_offset) as _,
            0,
            (to_write_len - buf_offset as u64) as _,
        );
    }
    pair.1 = Some(user_data);

    let write = IoWrite {
        file_index: file_offset.file_index,
        buf_index: iovec_index as u32,
        data_size: buf_offset as u32,
        file_offset: file_offset.offset,
    };
    let buf_num = iovec_buf.buf.len();
    iovec_buf.next_buf_index = (iovec_index + 1) % buf_num;
    iovec_buf.available_buf -= 1;
    file_offset.offset += buf_offset as u64;
    io_write.push(write);

    write_index
}

fn write_to_iovec<U: Clone + Debug + Send + 'static>(
    log_write: &mut Vec<(Buf, U)>,
    file_offset: &mut FileOffset,
    iovec_buf: &mut IoVecBuf<U>,
    io_write: &mut Vec<IoWrite>,
) {
    if log_write.is_empty() {
        return;
    }
    if iovec_buf.available_buf == 0 {
        return;
    }
    let mut opt_split_index = None;
    let mut to_write_index = 0usize;
    let total_to_write = log_write.len();
    while to_write_index < total_to_write {
        let mut index = iovec_buf.next_buf_index;
        let opt_buf_index = loop {
            if iovec_buf.buf[index].1.is_none() {
                break Some(index);
            }
            index = (index + 1) % iovec_buf.buf.len();
            if index == iovec_buf.next_buf_index {
                break None;
            }
        };
        match opt_buf_index {
            Some(buf_index) => {
                let n = write_to_iovec_one(
                    log_write,
                    file_offset,
                    iovec_buf,
                    io_write,
                    buf_index,
                    to_write_index,
                );
                to_write_index = n;
            }
            None => opt_split_index = Some(to_write_index),
        }
        to_write_index += 1;
    }
    match opt_split_index {
        Some(split_index) => {
            *log_write = log_write.split_off(split_index);
        }
        None => {
            log_write.clear();
        }
    }
}

impl IoUring {
    fn create(iovec: Vec<iovec>, fds: Vec<RawFd>, setting: &IOUSetting) -> Result<Self> {
        let mut ring: rliburing::io_uring = unsafe { std::mem::zeroed() };
        let mut param: rliburing::io_uring_params = unsafe { std::mem::zeroed() };
        param.flags = rliburing::IORING_SETUP_SQPOLL;
        param.sq_thread_idle = 2000;
        let r = unsafe {
            rliburing::io_uring_queue_init_params(setting.queue_size as _, &mut ring, &mut param)
        };
        if r != 0 {
            return Err(iou_error!("io_uring_queue_init_params error {}", r).into());
        }

        let r = unsafe {
            rliburing::io_uring_register_files(
                &mut ring,
                fds.as_slice().as_ptr() as _,
                fds.len() as _,
            )
        };
        if r != 0 {
            return Err(iou_error!("io_uring_register_files error {}", r).into());
        }

        let r = unsafe {
            rliburing::io_uring_register_buffers(&mut ring, iovec.as_ptr() as _, iovec.len() as _)
        };
        if r != 0 {
            return Err(iou_error!("io_uring_buffers error {}", r).into());
        }
        Ok(Self {
            ring,
            param,
            iovec,
            fds,
            align_byte: setting.sector_size,
        })
    }
}

async fn iou_event_loop_handle_gut<
    E: Send + 'static,
    U: Clone + Debug + Send + 'static,
    F: Fn(E) -> (Buf, U),
    C: Fn(Vec<U>),
>(
    receiver: Receiver<E>,
    iovec: Vec<iovec>,
    fds: Vec<RawFd>,
    to_user_data: &F,
    on_completion: &C,
    setting: &IOUSetting,
) -> Result<()> {
    let mut ring = IoUring::create(iovec, fds, setting)?;
    let mut n_submitted = 0;
    let mut write_log = vec![];
    let mut write_io = vec![];
    let mut has_poll_events = false;
    let mut receiver = receiver;
    let mut io_vec_buf = IoVecBuf {
        available_buf: ring.iovec.len(),
        next_buf_index: 0,
        align_byte: setting.sector_size,
        buf: ring.iovec.iter().map(|iov| (iov.clone(), None)).collect(),
    };
    let mut file_offset: Vec<FileOffset> = ring
        .fds
        .iter()
        .enumerate()
        .map(|(i, f)| FileOffset {
            file_fd: f.as_raw_fd() as u32,
            file_index: i as u32,
            offset: 0,
        })
        .collect();

    loop {
        if !has_poll_events && io_vec_buf.available_buf > 0 {
            let mut recv = vec![];
            let n = receiver
                .recv_many(&mut recv, io_vec_buf.available_buf as _)
                .await;
            if n == 0 {
                break;
            }
            let mut u_vec = vec![];
            for e in recv {
                let (buf, data) = to_user_data(e);
                u_vec.push(data.clone());
                write_log.push((buf, data));
            }
            debug!("append log {:?}", u_vec);
        } else {
            sleep(Duration::from_millis(2)).await;
        }
        write_to_iovec(
            &mut write_log,
            &mut file_offset[0],
            &mut io_vec_buf,
            &mut write_io,
        );

        let n = iou_submit(&mut ring, &mut write_io, &io_vec_buf)?;
        n_submitted += n;

        let complete = iou_complete(&mut ring, &mut io_vec_buf, n_submitted, on_completion)?;
        n_submitted -= complete;

        has_poll_events = n_submitted > 0 || !write_log.is_empty() || !write_io.is_empty()
    }
    Ok(())
}

fn iou_submit<U: Clone + Debug + Send + 'static>(
    ring: &mut IoUring,
    write_ops: &mut Vec<IoWrite>,
    io_vec_buf: &IoVecBuf<U>,
) -> Result<usize> {
    if write_ops.is_empty() {
        return Ok(0);
    }
    let mut submit_count: usize = 0;
    let mut opt_split_index = None; // if queue is full, this would be set
    for (n, op) in write_ops.iter().enumerate() {
        let buf_iovec = ring.iovec[op.buf_index as usize];

        let cqe = unsafe {
            let cqe = rliburing::io_uring_get_sqe(&mut ring.ring);
            cqe
        };
        if cqe == std::ptr::null_mut() {
            opt_split_index = Some(n);
            break;
        }
        unsafe {
            (*cqe).user_data = op.buf_index as _;
            rliburing::io_uring_prep_write_fixed(
                cqe,
                ring.fds[op.file_index as usize],
                buf_iovec.iov_base as _,
                op.data_size as _,
                op.file_offset as _,
                op.buf_index as _,
            )
        }
        debug!("submit {:?}", io_vec_buf.buf[op.buf_index as usize].1);
        submit_count += 1;
    }
    if let Some(n) = opt_split_index {
        *write_ops = write_ops.split_off(n);
    } else {
        write_ops.clear();
    }
    if submit_count > 0 {
        let n = unsafe { rliburing::io_uring_submit(&mut ring.ring) };
        if n as usize != submit_count {
            panic!("submit count error expeted {}, but {}", submit_count, n);
        }
    }
    Ok(submit_count)
}

fn iou_complete<U: Clone + Debug + Send + 'static, C: Fn(Vec<U>)>(
    ring: &mut IoUring,
    io_vec_buf: &mut IoVecBuf<U>,
    in_submitting: usize,
    on_completion: &C,
) -> Result<usize> {
    let mut complete = 0;
    if in_submitting == 0 {
        return Ok(0);
    }
    let mut user_data = vec![];
    for _i in 0..in_submitting {
        let mut cqe_ptr: *mut rliburing::io_uring_cqe = unsafe { std::mem::zeroed() };
        let r = unsafe { rliburing::io_uring_peek_cqe(&mut ring.ring, &mut cqe_ptr) };
        if r == -libc::EAGAIN {
            break;
        } else if r < 0 {
            return Err(iou_error!("io_uring_peek_cqe error {}", r));
        }
        let buf_index = unsafe { (*cqe_ptr).user_data } as usize;
        io_vec_buf.available_buf += 1;
        let pair = &mut io_vec_buf.buf[buf_index];
        let mut opt_user_data = None;
        std::mem::swap(&mut pair.1, &mut opt_user_data);
        user_data.extend(opt_user_data.unwrap());

        io_vec_buf.buf[buf_index].1 = None;
        unsafe { rliburing::io_uring_cqe_seen(&mut ring.ring, cqe_ptr) };
        complete += 1;
    }
    if !user_data.is_empty() {
        on_completion(user_data);
    }
    Ok(complete)
}

fn round_up_align(x: u64, size: u64) -> u64 {
    let mask = size - 1;
    if x & mask == 0 {
        x & !mask
    } else {
        (x & !mask) + size
    }
}

async fn _io_uring_event_loop<
    E: Send + 'static,
    U: Clone + Debug + Send + 'static,
    F: Fn(E) -> (Buf, U),
    C: Fn(Vec<U>),
>(
    file_path: Vec<String>,
    receiver: IoChRecv<E>,
    to_user_data: F,
    on_completion: C,
    setting: IOUSetting,
) -> Result<()> {
    let mut files = vec![];
    let mut fds = vec![];
    let mut buf_iovec = vec![];
    let _mmap_ptr = unsafe {
        let sector_size = setting.sector_size;
        let buf_size = round_up_align(setting.buffer_size, sector_size);
        let size = setting.buffer_count * buf_size;
        let ptr = libc::mmap(
            0 as _,
            size as usize,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        for _i in 0..setting.buffer_count {
            buf_iovec.push(iovec {
                iov_base: ptr.add((_i * buf_size) as usize),
                iov_len: buf_size as usize,
            });
        }
        ptr
    };

    for p in file_path {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .custom_flags(libc::O_DIRECT)
            .open(&p)?;
        fds.push(file.as_raw_fd());
        files.push(file);
    }

    iou_event_loop_handle(
        receiver,
        buf_iovec,
        fds,
        to_user_data,
        on_completion,
        setting,
    )
        .await?;
    Ok(())
}
