use crate::common::bc::{hdr_size, tail_size, BCHdr, BCTail};
use crate::common::bc_dec::{DecErr, Decoder};
use crate::common::buf::Buf;
use crate::common::crc::calc_crc;
use crate::common::endian::Endian;
use crate::common::slice::SliceMutRef;
use byteorder::ByteOrder;

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
pub enum EncErr {
    CapacityNotAvailable,
}

pub trait Encoder {
    fn write_i8(&mut self, n: i8) -> Result<(), EncErr> {
        let a = [n];
        let bytes = unsafe { &*(&a as *const [i8] as *const [u8]) };
        self.write(bytes)?;
        Ok(())
    }

    fn write_u8(&mut self, n: u8) -> Result<(), EncErr> {
        self.write(&[n])?;
        Ok(())
    }

    fn write_i32(&mut self, n: i32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_i32(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_u32(&mut self, n: u32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_u32(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_i64(&mut self, n: i64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_i64(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_u64(&mut self, n: u64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_u64(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_i128(&mut self, n: i128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_i128(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_u128(&mut self, n: u128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_u128(&mut buf, n);
        self.write(&buf)?;
        Ok(())
    }

    fn write_bytes(&mut self, s: &[u8]) -> Result<(), EncErr> {
        self.write(s)
    }

    fn write(&mut self, s: &[u8]) -> Result<(), EncErr>;
}

pub trait Encode {
    /// Encode a given type.
    fn encode<E: Encoder>(&self, ncoder: &mut E) -> Result<(), EncErr>;

    fn size(&self) -> Result<usize, EncErr>;
}

const DEFAULT_BUF_SIZE: usize = 1024;

pub fn encode_binary<E: Encode>(e: &E) -> Result<Buf, EncErr> {
    let n = DEFAULT_BUF_SIZE + hdr_size() + tail_size();
    let mut buf: Buf = vec![0; n];
    let r = _binary_encode(e, &mut buf)?;
    match r {
        Ok(()) => Ok(buf),
        Err(size) => {
            buf.resize(size + hdr_size() + tail_size(), 0);
            let _r = _binary_encode(e, &mut buf)?;
            match _r {
                Ok(()) => Ok(buf),
                Err(_) => {
                    buf.resize(size + hdr_size() + tail_size(), 0);
                    let _r = _binary_encode(e, &mut buf)?;
                    panic!("error capacity");
                }
            }
        }
    }
}

fn _binary_encode<E: Encode>(e: &E, buf: &mut Buf) -> Result<Result<(), usize>, EncErr> {
    let header_size = hdr_size();
    if buf.len() < header_size {
        return Ok(Err(e.size()?));
    };

    let buf_len = buf.len();
    let mut s = SliceMutRef::new(&mut buf.as_mut_slice()[header_size..buf_len]);
    let r = e.encode(&mut s);
    match r {
        Ok(()) => {
            let body_size = s.write_pos();
            let _ = s;

            let size = header_size + tail_size() + body_size;

            buf.resize(size, 0);
            let crc = calc_crc(&buf.as_slice()[header_size..header_size + body_size]);
            {
                let hdr = BCHdr::new(body_size as u32, crc);
                let mut s1 = SliceMutRef::new(&mut buf.as_mut_slice()[0..header_size]);
                hdr.encode(&mut s1)?;
            }
            {
                let mut s2 = SliceMutRef::new(&mut buf.as_mut_slice()[header_size + body_size..]);
                let tail = BCTail::new(crc);
                tail.encode(&mut s2)?;
            }
            Ok(Ok(()))
        }
        Err(err) => match err {
            EncErr::CapacityNotAvailable => Ok(Err(e.size()?)),
        },
    }
}

impl Encoder for Buf {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncErr> {
        self.extend(bytes);
        Ok(())
    }
}

impl Decoder for (Buf, usize) {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecErr> {
        if self.0.len() >= self.1 + bytes.len() {
            bytes.copy_from_slice(&self.0[self.1..self.1 + bytes.len()]);
            self.1 += bytes.len();
            Ok(())
        } else {
            Err(DecErr::CapacityNotAvailable)
        }
    }
}
