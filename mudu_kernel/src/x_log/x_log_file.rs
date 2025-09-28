use mudu::common::bc_dec::decode_binary;
use mudu::common::buf::{resize_buf, Buf};
use regex::Regex;
use std::cmp::min;

use std::fs::File as StdFile;
use std::path::PathBuf;
use std::str::FromStr;

use crate::contract::lsn::LSN;
use crate::contract::xl_batch::XLBatch;
use crate::contract::xl_chunk::{
    decode_chunk, decode_chunk_hdr, write_chunk_to_u_file, ChunkHdr, ChunkTail, XLChunk,
    XLChunkType, LOG_C_COMMON_HDR_SIZE, LOG_C_TAIL_SIZE,
};
use crate::io::file::{File, TFile};
use crate::x_log::lsn_syncer::LSNSyncer;
use crate::x_log::xl_cfg::XLCfg;
use crate::x_log::xl_file_info::XLFileInfo;
use crate::x_log::xl_path;
use crate::x_log::xl_path::xl_file_path;
use mudu::common::result::RS;
use mudu::common::result_of::std_io_error;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::task_trace;
use tokio::fs::read_dir;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::Receiver;
use tracing::error;

pub struct XLogFile {
    conf: XLCfg,
    file: AsyncFile,
}

struct AsyncFile {
    channel_name: String,
    log_file_seq_no: u32,
    buf: Vec<u8>,
    file: File,
    file_size: u64,
}

async fn new_file(folder: &String, name: &String, ext: &String, no: u32) -> RS<File> {
    let path = PathBuf::from(folder);
    let path = path.join(xl_path::format_xl_file_name(name, ext, no));
    let _r = File::create(path).await;
    let file = std_io_error(_r)?;
    Ok(file)
}

impl AsyncFile {
    fn new(channel_name: String, file: File, file_size: u64, log_file_seq_no: u32) -> AsyncFile {
        Self {
            channel_name,
            log_file_seq_no,
            buf: Default::default(),
            file,
            file_size,
        }
    }

    async fn fsync(&mut self) -> RS<()> {
        let r = self.file.sync_all().await;
        std_io_error(r)
    }

    async fn write_all(&mut self, lsn: LSN, buf: Buf, cfg: &XLCfg) -> RS<()> {
        let _trace = task_trace!();
        if self.file_size > cfg.log_file_size_limit() {
            panic!("size limit exceeded");
        }
        let mut write_buf_offset = 0;
        let capacity_low =
            self.file_size as usize + ChunkHdr::size_of() + ChunkTail::size_of() + buf.len()
                > cfg.log_file_size_limit() as usize;
        let mut file_offset = self.file_size;
        if capacity_low {
            let mut seq = 0;
            let mut last_chunk = false;
            while buf.len() > write_buf_offset {
                let possible_write_len =
                    cfg.log_file_size_limit() as usize - self.file_size as usize;
                if possible_write_len > ChunkHdr::size_of() + ChunkTail::size_of() {
                    let len1 = possible_write_len - ChunkHdr::size_of() - ChunkTail::size_of();
                    let len = min(len1, buf.len() - write_buf_offset);
                    let _buf = &buf[write_buf_offset..write_buf_offset + len];
                    let size =
                        write_chunk_to_u_file(&mut self.file, lsn, _buf, Some((seq, last_chunk)))
                            .await?;
                    write_buf_offset += _buf.len();
                    self.file_size += size as u64;
                    seq += 1;
                    last_chunk = buf.len() == write_buf_offset;
                    self.fsync().await?;
                }
                self.file_size = 0;
                self.log_file_seq_no += 1;
                let file = new_file(
                    cfg.log_path(),
                    &self.channel_name,
                    cfg.log_ext_name(),
                    self.log_file_seq_no,
                )
                    .await?;
                self.file = file;
            }
        } else {
            let size = write_chunk_to_u_file(&mut self.file, lsn, &buf, None).await?;
            file_offset += size as u64;
            write_buf_offset += size;
            self.file_size = file_offset;
        }

        Ok(())
    }
}

impl XLogFile {
    pub fn new(cfg: XLCfg, channel_name: String, file_size: u64, log_file_no: u32) -> RS<XLogFile> {
        let path = xl_file_path(
            &cfg.x_log_path,
            &channel_name,
            &cfg.x_log_ext_name,
            log_file_no,
        );
        let r = std::fs::OpenOptions::new().write(true).open(path);
        let file = std_io_error(r)?;
        let async_file = File::from_std(file);
        Ok(Self {
            conf: cfg,
            file: AsyncFile::new(channel_name, async_file, file_size, log_file_no),
        })
    }

    pub fn file_info(&self) -> XLFileInfo {
        XLFileInfo {
            cfg: self.conf.clone(),
            file_no: self.file.log_file_seq_no,
            file_size: self.file.file_size,
            channel_name: self.file.channel_name.clone(),
        }
    }

    pub async fn recovery(conf: XLCfg, name: String) -> RS<Self> {
        let _trace = task_trace!();
        let min = LOG_C_COMMON_HDR_SIZE * 10usize;
        let max = i32::MAX as usize;
        if (conf.x_log_file_size_limit as usize) < min
            && (conf.x_log_file_size_limit as usize) > max
        {
            panic!("log file size limit must between {} {}", min, max);
        }

        Self::recovery_gut(conf, name).await
    }

    pub async fn f_sync_loop(
        &mut self,
        receiver: Receiver<(Buf, u64)>,
        lsn_syncer: LSNSyncer,
    ) -> RS<()> {
        let _trace = task_trace!();
        let r = self.f_sync_loop_gut(receiver, lsn_syncer).await;
        match r {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("fsync loop error: {}", e);
                Err(e)
            }
        }
    }

    async fn recovery_gut(conf: XLCfg, name: String) -> RS<Self> {
        let _trace = task_trace!();
        let regex =
            Regex::new(format!(r"^{}_([0-9]+).{}$", name, &conf.x_log_ext_name).as_str()).unwrap();
        let _r = read_dir(&conf.x_log_path).await;
        let mut read_dir = std_io_error(_r)?;
        let mut vec_no = Vec::new();
        loop {
            let _r = read_dir.next_entry().await;
            let opt = std_io_error(_r)?;
            if let Some(entry) = opt {
                let name = entry.file_name().to_str().unwrap().to_string();
                let _rc = regex.captures(&name);
                if let Some(c) = _rc {
                    if let Some(m) = c.get(1) {
                        let no = u32::from_str(m.as_str()).unwrap();
                        vec_no.push(no);
                    }
                }
            } else {
                break;
            }
        }
        vec_no.sort();

        let (seq_no, file, size) = if let Some(no) = vec_no.last() {
            let file = Self::open_std_file(&conf.x_log_path, &name, &conf.x_log_ext_name, *no)?;
            let _r = file.metadata();
            let meta = std_io_error(_r)?;
            let size = meta.len();
            if size > conf.x_log_file_size_limit {
                panic!("file size limit exceeded");
            }
            (*no, File::from_std(file), size)
        } else {
            (
                1,
                new_file(&conf.x_log_path, &name, &conf.x_log_ext_name, 1).await?,
                0,
            )
        };

        let s = Self {
            conf,
            file: AsyncFile::new(name, file, size, seq_no),
        };
        Ok(s)
    }

    fn open_std_file(folder: &String, name: &String, ext: &String, no: u32) -> RS<StdFile> {
        let _trace = task_trace!();
        let path = PathBuf::from(folder);
        let path = path.join(xl_path::format_xl_file_name(name, ext, no));
        //info!("open file {}", path.as_path().display());
        let _r = StdFile::open(path);
        let file = std_io_error(_r)?;
        Ok(file)
    }

    // if prev_seq_no == 0, then,

    fn consolidate_part(vec: Vec<Buf>) -> RS<XLBatch> {
        let mut buf = Buf::new();
        for v in vec {
            buf.extend(v);
        }
        let _r = decode_binary::<XLBatch>(&buf);
        let l = match _r {
            Ok(l) => l,
            Err(e) => {
                return Err(m_error!(ER::DecodeErr, "", e));
            }
        };
        Ok(l)
    }

    async fn read_from_all(
        conf: &XLCfg,
        name: &String,
        log_file_ids: Vec<u32>,
    ) -> RS<Vec<XLBatch>> {
        let _trace = task_trace!();
        let mut res = vec![];
        let mut prev_part: Vec<Buf> = Vec::<Buf>::new();
        for id in log_file_ids.iter() {
            let r = Self::read_from_log_file(conf, name, *id, false).await?;
            let parts = match r {
                Ok(vec) => vec,
                Err(_) => {
                    panic!("log entry cannot start from log part ");
                }
            };
            for part in parts {
                match part {
                    XLChunk::Part(p) => {
                        prev_part.extend(p);
                    }
                    XLChunk::Whole(l) => {
                        if prev_part.len() > 1 {
                            let mut part_vec = Vec::new();
                            std::mem::swap(&mut part_vec, &mut prev_part);
                            let xl = Self::consolidate_part(part_vec)?;
                            res.push(xl);
                        }
                        res.push(l);
                    }
                }
            }
        }
        Ok(res)
    }
    async fn read_to_buf(conf: &XLCfg, name: &String, log_file_id: u32) -> RS<Buf> {
        let _trace = task_trace!();
        let std_file =
            Self::open_std_file(&conf.x_log_path, name, &conf.x_log_ext_name, log_file_id)?;
        let mut file = TFile::from_std(std_file);
        let mut buf = Buf::new();
        resize_buf(&mut buf, conf.x_log_file_size_limit as usize);
        let _r = file.read_to_end(&mut buf).await;
        let n = std_io_error(_r)?;
        buf.resize(n, 0);
        Ok(buf)
    }

    async fn read_from_log_file(
        conf: &XLCfg,
        name: &String,
        log_file_id: u32,
        read_first_part: bool,
    ) -> RS<Result<Vec<XLChunk>, u32>> {
        let _trace = task_trace!();
        let mut buf = Self::read_to_buf(conf, name, log_file_id).await?;
        let _buf_slice = &mut buf.as_mut_slice();
        if read_first_part {
            let _r = decode_chunk_hdr(_buf_slice);
            if let Ok(_hdr) = _r {
                // todo
            } else {
                panic!("todo, corrupt log");
            }
        }
        let (vec, _) = Self::read_from_buf(_buf_slice)?;
        Ok(Ok(vec))
    }

    fn read_from_buf(slice: &[u8]) -> RS<(Vec<XLChunk>, usize)> {
        let mut offset = 0usize;
        let mut result = vec![];
        while offset < slice.len() {
            let slice = &slice[offset..];
            let _r = decode_chunk(slice);
            let (hdr, body) = match _r {
                Ok((h, body)) => (h, body),
                Err(e) => {
                    // todo! corrupted log
                    return Err(m_error!(ER::DecodeErr, "", e));
                }
            };

            let header_size = ChunkHdr::size_of();
            let body_size = hdr.body_length() as usize;
            offset += header_size + body_size + LOG_C_TAIL_SIZE;
            let xl_part = match hdr.chunk_type() {
                XLChunkType::Part => {
                    let body_buf = Buf::from(body);
                    XLChunk::Part(vec![body_buf])
                }
                XLChunkType::Whole => {
                    let _r = decode_binary::<XLBatch>(body);
                    let xl_batch = match _r {
                        Ok(l) => l,
                        Err(_e) => {
                            // log corrupt
                            return Err(m_error!(ER::DecodeErr, "", _e));
                        }
                    };
                    XLChunk::Whole(xl_batch)
                }
            };
            result.push(xl_part);
        }
        assert_eq!(slice.len(), offset);
        Ok((result, offset))
    }

    async fn f_sync_loop_gut(
        &mut self,
        receiver: Receiver<(Buf, u64)>,
        lsn_syncer: LSNSyncer,
    ) -> RS<()> {
        let _trace = task_trace!();
        let mut receiver = receiver;
        loop {
            let mut vec = vec![];
            let mut ready_lsn = vec![];
            let r = receiver.recv_many(&mut vec, 10).await;
            if r == 0 {
                break;
            }
            for (buf, lsn) in vec {
                //info!("handle lsn {}", lsn);
                self.file.write_all(lsn, buf, &self.conf).await?;
                ready_lsn.push(lsn);
                //info!("handle lsn {} done", lsn);
            }
            self.file.fsync().await?;
            //info!("ready lsn {:?}", ready_lsn);
            lsn_syncer.ready(ready_lsn);
        }
        Ok(())
    }
}

unsafe impl Sync for XLogFile {}
unsafe impl Send for XLogFile {}
