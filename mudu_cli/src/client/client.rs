use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    ClientRequest, Frame, GetRequest, HEADER_LEN, KeyValue, MessageType, ProcedureInvokeRequest,
    PutRequest, RangeScanRequest, ServerResponse, SessionCloseRequest, SessionCreateRequest,
    decode_error_response, decode_get_response, decode_procedure_invoke_response,
    decode_put_response, decode_range_scan_response, decode_server_response,
    decode_session_close_response, decode_session_create_response, encode_client_request,
    encode_client_request_with_message_type, encode_get_request, encode_procedure_invoke_request,
    encode_put_request, encode_range_scan_request, encode_session_close_request,
    encode_session_create_request,
};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

pub struct SyncClient {
    stream: TcpStream,
    next_request_id: u64,
}

impl SyncClient {
    pub fn connect<A: ToSocketAddrs>(addr: A) -> RS<Self> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| m_error!(EC::NetErr, "connect io_uring tcp server error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set tcp nodelay error", e))?;
        Ok(Self {
            stream,
            next_request_id: 1,
        })
    }

    pub fn query(
        &mut self,
        app_name: impl Into<String>,
        sql: impl Into<String>,
    ) -> RS<ServerResponse> {
        let request_id = self.take_request_id();
        let request = ClientRequest::new(app_name, sql);
        let payload = encode_client_request(request_id, &request)?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        decode_server_response(&frame)
    }

    pub fn execute(
        &mut self,
        app_name: impl Into<String>,
        sql: impl Into<String>,
    ) -> RS<ServerResponse> {
        let request_id = self.take_request_id();
        let request = ClientRequest::new(app_name, sql);
        let payload =
            encode_client_request_with_message_type(MessageType::Execute, request_id, &request)?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        decode_server_response(&frame)
    }

    pub fn get(&mut self, session_id: u128, key: impl Into<Vec<u8>>) -> RS<Option<Vec<u8>>> {
        let request_id = self.take_request_id();
        let payload = encode_get_request(request_id, &GetRequest::new(session_id, key.into()))?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_get_response(&frame)?.into_value())
    }

    pub fn put(
        &mut self,
        session_id: u128,
        key: impl Into<Vec<u8>>,
        value: impl Into<Vec<u8>>,
    ) -> RS<()> {
        let request_id = self.take_request_id();
        let payload = encode_put_request(
            request_id,
            &PutRequest::new(session_id, key.into(), value.into()),
        )?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        if decode_put_response(&frame)?.ok() {
            Ok(())
        } else {
            Err(m_error!(
                EC::NetErr,
                "remote put operation returned failure"
            ))
        }
    }

    pub fn range_scan(
        &mut self,
        session_id: u128,
        start_key: impl Into<Vec<u8>>,
        end_key: impl Into<Vec<u8>>,
    ) -> RS<Vec<KeyValue>> {
        let request_id = self.take_request_id();
        let payload = encode_range_scan_request(
            request_id,
            &RangeScanRequest::new(session_id, start_key.into(), end_key.into()),
        )?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_range_scan_response(&frame)?.into_items())
    }

    pub fn invoke_procedure(
        &mut self,
        session_id: u128,
        procedure_name: impl Into<String>,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>> {
        let request_id = self.take_request_id();
        let payload = encode_procedure_invoke_request(
            request_id,
            &ProcedureInvokeRequest::new(session_id, procedure_name, procedure_parameters),
        )?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_procedure_invoke_response(&frame)?.into_result())
    }

    pub fn create_session(&mut self, config_json: Option<String>) -> RS<u128> {
        let request_id = self.take_request_id();
        let payload =
            encode_session_create_request(request_id, &SessionCreateRequest::new(config_json))?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_session_create_response(&frame)?.session_id())
    }

    pub fn close_session(&mut self, session_id: u128) -> RS<bool> {
        let request_id = self.take_request_id();
        let payload =
            encode_session_close_request(request_id, &SessionCloseRequest::new(session_id))?;
        let frame = self.send_and_receive(&payload)?;
        self.ensure_success_frame(&frame)?;
        Ok(decode_session_close_response(&frame)?.closed())
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    fn send_and_receive(&mut self, payload: &[u8]) -> RS<Frame> {
        self.stream
            .write_all(payload)
            .map_err(|e| m_error!(EC::NetErr, "write request frame error", e))?;
        self.stream
            .flush()
            .map_err(|e| m_error!(EC::NetErr, "flush request frame error", e))?;

        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .map_err(|e| m_error!(EC::NetErr, "read response header error", e))?;
        let payload_len =
            u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
        let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
        frame_bytes.extend_from_slice(&header);
        if payload_len > 0 {
            let mut body = vec![0u8; payload_len];
            self.stream
                .read_exact(&mut body)
                .map_err(|e| m_error!(EC::NetErr, "read response payload error", e))?;
            frame_bytes.extend_from_slice(&body);
        }
        Frame::decode(&frame_bytes)
    }

    fn ensure_success_frame(&self, frame: &Frame) -> RS<()> {
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(frame)?;
            return Err(m_error!(EC::NetErr, error.message()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::protocol::{
        GetResponse, ProcedureInvokeResponse, PutResponse, RangeScanResponse, SessionCloseResponse,
        SessionCreateResponse, encode_get_response, encode_procedure_invoke_response,
        encode_put_response, encode_range_scan_response, encode_session_close_response,
        encode_session_create_response,
    };
    use std::net::TcpListener;
    use std::thread;

    fn bind_test_listener() -> Option<TcpListener> {
        match TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => Some(listener),
            Err(err) => {
                eprintln!("skip tcp client test: {err}");
                None
            }
        }
    }

    #[test]
    fn client_get_roundtrip() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            assert_eq!(frame.header().request_id(), 1);

            let response =
                encode_get_response(1, &GetResponse::new(Some(b"value-1".to_vec()))).unwrap();
            socket.write_all(&response).unwrap();
        });

        let mut client = SyncClient::connect(addr).unwrap();
        let response = client.get(7, b"key-1".to_vec()).unwrap();
        assert_eq!(response, Some(b"value-1".to_vec()));
        server.join().unwrap();
    }

    #[test]
    fn client_put_and_range_scan_decode() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();

            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            let response =
                encode_put_response(frame.header().request_id(), &PutResponse::new(true)).unwrap();
            socket.write_all(&response).unwrap();

            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            let response = encode_range_scan_response(
                frame.header().request_id(),
                &RangeScanResponse::new(vec![
                    KeyValue::new(b"a".to_vec(), b"1".to_vec()),
                    KeyValue::new(b"b".to_vec(), b"2".to_vec()),
                ]),
            )
            .unwrap();
            socket.write_all(&response).unwrap();
        });

        let mut client = SyncClient::connect(addr).unwrap();
        client.put(7, b"k".to_vec(), b"v".to_vec()).unwrap();
        let items = client.range_scan(7, b"a".to_vec(), b"z".to_vec()).unwrap();
        assert_eq!(
            items,
            vec![
                KeyValue::new(b"a".to_vec(), b"1".to_vec()),
                KeyValue::new(b"b".to_vec(), b"2".to_vec()),
            ]
        );
        server.join().unwrap();
    }

    #[test]
    fn client_procedure_invoke_decode() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            let response = encode_procedure_invoke_response(
                frame.header().request_id(),
                &ProcedureInvokeResponse::new(b"done".to_vec()),
            )
            .unwrap();
            socket.write_all(&response).unwrap();
        });

        let mut client = SyncClient::connect(addr).unwrap();
        let result = client
            .invoke_procedure(11, "app/mod/proc", b"params".to_vec())
            .unwrap();
        assert_eq!(result, b"done".to_vec());
        server.join().unwrap();
    }

    #[test]
    fn client_session_lifecycle_decode() {
        let Some(listener) = bind_test_listener() else {
            return;
        };
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();

            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            let response = encode_session_create_response(
                frame.header().request_id(),
                &SessionCreateResponse::new(99),
            )
            .unwrap();
            socket.write_all(&response).unwrap();

            let mut header = [0u8; HEADER_LEN];
            socket.read_exact(&mut header).unwrap();
            let payload_len =
                u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
            let mut body = vec![0u8; payload_len];
            socket.read_exact(&mut body).unwrap();
            let mut request = Vec::from(header);
            request.extend_from_slice(&body);
            let frame = Frame::decode(&request).unwrap();
            let response = encode_session_close_response(
                frame.header().request_id(),
                &SessionCloseResponse::new(true),
            )
            .unwrap();
            socket.write_all(&response).unwrap();
        });

        let mut client = SyncClient::connect(addr).unwrap();
        let session_id = client
            .create_session(Some("{\"partition\":1}".to_string()))
            .unwrap();
        assert_eq!(session_id, 99);
        let closed = client.close_session(session_id).unwrap();
        assert!(closed);
        server.join().unwrap();
    }
}
