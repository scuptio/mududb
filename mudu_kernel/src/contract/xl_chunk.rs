use crate::contract::lsn::LSN;
use crate::contract::xl_batch::XLBatch;
use crate::io::file::File;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};
use mudu::common::buf::Buf;
use mudu::common::crc::calc_crc;
use mudu::common::result::RS;
use mudu::common::result_of::std_io_error;
use mudu::common::slice::{SliceMutRef, SliceRef};
use mudu_utils::task_trace;
use tokio::io::AsyncWriteExt;

pub const LOG_CHUNK_PART: u8 = 1u8;
pub const LOG_CHUNK_WHOLE: u8 = 2u8;
pub const LOG_C_TYPE_SIZE: usize = size_of::<u8>();
pub const LOG_C_CRC_SIZE: usize = size_of::<u64>();
pub const LOG_C_COMMON_HDR_SIZE: usize = LOG_C_TYPE_SIZE + LOG_C_CRC_SIZE + size_of::<u32>();
pub const LOG_C_HDR_SEQ_SIZE: usize = size_of::<u32>();
pub const LOG_C_PART_HDR_SIZE: usize = LOG_C_COMMON_HDR_SIZE + LOG_C_HDR_SEQ_SIZE;
pub const LOG_C_TAIL_SIZE: usize = LOG_C_CRC_SIZE;

const CHUNK_LAST_MASK: u32 = 1u32 << 31;
pub struct ChunkHdr {
    lsn: u64,
    body_crc: u64,
    body_length: u32,
    // first 1 bit, is this chunk the last?
    // last chunk 31 bit sequence
    chunk_seq: u32,
}

pub struct ChunkTail {
    body_crc: u64,
}

pub enum XLChunk {
    Part(Vec<Buf>),
    Whole(XLBatch),
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub enum XLChunkType {
    Part,
    Whole,
}

impl XLChunkType {
    pub fn from(t: u8) -> XLChunkType {
        match t {
            LOG_CHUNK_PART => XLChunkType::Part,
            LOG_CHUNK_WHOLE => XLChunkType::Whole,
            _ => {
                panic!("unknown enum value")
            }
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            XLChunkType::Part => LOG_CHUNK_PART,
            XLChunkType::Whole => LOG_CHUNK_WHOLE,
        }
    }

    pub fn hdr_size(&self) -> usize {
        match self {
            XLChunkType::Part => LOG_C_COMMON_HDR_SIZE,
            XLChunkType::Whole => LOG_C_PART_HDR_SIZE,
        }
    }
}

pub async fn write_chunk_to_u_file(
    file: &mut File,
    lsn: LSN,
    body: &[u8],
    chunk_seq: Option<(u32, bool)>,
) -> RS<usize> {
    let _trace = task_trace!();
    const HEADER_SIZE: usize = LOG_C_COMMON_HDR_SIZE + LOG_C_HDR_SEQ_SIZE;
    const TAIL_SIZE: usize = LOG_C_TAIL_SIZE;
    let mut buf: [u8; HEADER_SIZE + TAIL_SIZE] = [0; HEADER_SIZE + TAIL_SIZE];
    let crc = calc_crc(body);

    let hdr = ChunkHdr::new(lsn, crc, body.len() as u32, chunk_seq);
    let tail = ChunkTail::new(crc);

    // write header
    let mut bf1 = SliceMutRef::new(&mut buf);
    let _ = hdr.encode(&mut bf1).unwrap();
    let mut size = bf1.as_slice().len();
    let r = file.write_all(bf1.as_slice()).await;
    std_io_error(r)?;

    // write body
    size += body.len();
    let r = file.write_all(body).await;
    std_io_error(r)?;

    // write tail
    let mut bf2 = SliceMutRef::new(&mut buf);
    let _ = tail.encode(&mut bf2).unwrap();
    size += bf2.as_slice().len();
    let r = file.write_all(bf2.as_slice()).await;
    std_io_error(r)?;

    Ok(size)
}

pub fn decode_chunk(buf: &[u8]) -> Result<(ChunkHdr, &[u8]), DecErr> {
    let h = decode_chunk_hdr(buf)?;
    let hdr_size = ChunkHdr::size_of();
    let body = &buf[hdr_size..hdr_size + h.body_length() as usize];
    let t = decode_chunk_tail(&buf[hdr_size + h.body_length() as usize..])?;
    if t.body_crc() != h.body_crc() {
        return Err(DecErr::ErrorCRC);
    }
    Ok((h, body))
}

fn decode_chunk_tail(buf: &[u8]) -> Result<ChunkTail, DecErr> {
    let mut r = SliceRef::new(buf);
    let c = ChunkTail::decode(&mut r)?;
    Ok(c)
}

pub fn decode_chunk_hdr(buf: &[u8]) -> Result<ChunkHdr, DecErr> {
    let mut r = SliceRef::new(buf);
    let c = ChunkHdr::decode(&mut r)?;
    Ok(c)
}

impl ChunkHdr {
    pub fn new(lsn: LSN, body_crc: u64, body_length: u32, chunk_seq: Option<(u32, bool)>) -> Self {
        let v = match chunk_seq {
            Some((seq, last)) => {
                if last {
                    seq | CHUNK_LAST_MASK
                } else {
                    seq
                }
            }
            None => u32::MAX,
        };
        Self {
            body_length,
            body_crc,
            lsn,
            chunk_seq: v,
        }
    }

    pub fn chunk_seq(&self) -> u32 {
        // clear first bit
        self.chunk_seq & !CHUNK_LAST_MASK
    }

    pub fn chunk_type(&self) -> XLChunkType {
        if self.chunk_seq == u32::MAX {
            XLChunkType::Whole
        } else {
            XLChunkType::Part
        }
    }

    pub fn lsn(&self) -> LSN {
        self.lsn
    }
    pub fn body_length(&self) -> u32 {
        self.body_length
    }

    pub fn body_crc(&self) -> u64 {
        self.body_crc
    }

    pub fn size_of() -> usize {
        size_of::<Self>()
    }
}

impl Decode for ChunkHdr {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr> {
        let lsn = decoder.read_u64()?;
        let body_crc = decoder.read_u64()?;
        let body_length = decoder.read_u32()?;
        let pad = decoder.read_u32()?;
        Ok(Self {
            lsn,
            body_length,
            body_crc,
            chunk_seq: pad,
        })
    }
}

impl Encode for ChunkHdr {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u64(self.lsn)?;
        encoder.write_u64(self.body_crc)?;
        encoder.write_u32(self.body_length)?;
        encoder.write_u32(self.chunk_seq)?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut size = 0;
        size += size_of_val(&self.lsn);
        size += size_of_val(&self.body_crc);
        size += size_of_val(&self.body_length);
        size += size_of_val(&self.chunk_seq);
        Ok(size)
    }
}

impl ChunkTail {
    fn new(body_crc: u64) -> Self {
        Self { body_crc }
    }

    pub fn body_crc(&self) -> u64 {
        self.body_crc
    }

    pub fn size_of() -> usize {
        size_of::<u64>()
    }
}

impl Decode for ChunkTail {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr> {
        let body_crc = decoder.read_u64()?;
        Ok(Self { body_crc })
    }
}

impl Encode for ChunkTail {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u64(self.body_crc)?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        Ok(LOG_C_CRC_SIZE)
    }
}
