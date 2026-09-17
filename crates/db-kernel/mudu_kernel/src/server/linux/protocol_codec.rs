use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{Frame, FrameHeader, HEADER_LEN};
use mudu_sys::io::socket::{recv_into, send_all, IoSocket};

pub(in crate::server) async fn read_next_frame(
    socket: &IoSocket,
    read_buf: &mut Vec<u8>,
) -> RS<Option<Frame>> {
    mudu_utils::scoped_task_trace!();
    let mut header_buf = [0u8; HEADER_LEN];
    match read_exact(socket, &mut header_buf).await? {
        Some(()) => {}
        None => return Ok(None),
    }
    let header = FrameHeader::decode_header_bytes(&header_buf)?;
    read_buf.clear();
    read_buf.resize(header.payload_len() as usize, 0);
    if !read_buf.is_empty() {
        read_exact(socket, read_buf.as_mut_slice())
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Parse,
                    "connection closed with an incomplete protocol frame"
                )
            })?;
    }
    let payload = std::mem::take(read_buf);
    Ok(Some(Frame::from_parts(header, payload)?))
}

pub(in crate::server) async fn write_response(socket: &IoSocket, payload: &[u8]) -> RS<()> {
    mudu_utils::scoped_task_trace!();
    send_all(socket, payload).await
}

async fn read_exact(socket: &IoSocket, mut dst: &mut [u8]) -> RS<Option<()>> {
    mudu_utils::scoped_task_trace!();
    let mut read_any = false;
    while !dst.is_empty() {
        let read = recv_into(socket, dst, 0).await?;
        if read == 0 {
            if read_any {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "connection closed with an incomplete protocol frame"
                ));
            }
            return Ok(None);
        }
        read_any = true;
        dst = &mut dst[read..];
    }
    Ok(Some(()))
}
