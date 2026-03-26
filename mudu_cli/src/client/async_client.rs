use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    ClientRequest, Frame, GetRequest, GetResponse, HEADER_LEN, MessageType, ProcedureInvokeRequest,
    ProcedureInvokeResponse, PutRequest, PutResponse, RangeScanRequest, RangeScanResponse,
    ServerResponse, SessionCloseRequest, SessionCloseResponse, SessionCreateRequest,
    SessionCreateResponse, decode_error_response, decode_get_response,
    decode_procedure_invoke_response, decode_put_response, decode_range_scan_response,
    decode_server_response, decode_session_close_response, decode_session_create_response,
    encode_client_request_with_message_type, encode_get_request, encode_procedure_invoke_request,
    encode_put_request, encode_range_scan_request, encode_session_close_request,
    encode_session_create_request,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[async_trait(?Send)]
pub trait AsyncClient: Send {
    async fn query(&mut self, request: ClientRequest) -> RS<ServerResponse>;
    async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse>;
    async fn get(&mut self, request: GetRequest) -> RS<GetResponse>;
    async fn put(&mut self, request: PutRequest) -> RS<PutResponse>;
    async fn range_scan(&mut self, request: RangeScanRequest) -> RS<RangeScanResponse>;
    async fn invoke_procedure(
        &mut self,
        request: ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse>;
    async fn create_session(&mut self, request: SessionCreateRequest) -> RS<SessionCreateResponse>;
    async fn close_session(&mut self, request: SessionCloseRequest) -> RS<SessionCloseResponse>;
}

pub struct AsyncClientImpl {
    stream: TcpStream,
    next_request_id: u64,
}

impl AsyncClientImpl {
    pub async fn connect(addr: &str) -> RS<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| m_error!(EC::NetErr, "connect io_uring tcp server error", e))?;
        stream
            .set_nodelay(true)
            .map_err(|e| m_error!(EC::NetErr, "set tcp nodelay error", e))?;
        Ok(Self {
            stream,
            next_request_id: 1,
        })
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    async fn send_and_receive(&mut self, payload: &[u8]) -> RS<Frame> {
        self.stream
            .write_all(payload)
            .await
            .map_err(|e| m_error!(EC::NetErr, "write request frame error", e))?;
        self.stream
            .flush()
            .await
            .map_err(|e| m_error!(EC::NetErr, "flush request frame error", e))?;

        let mut header = [0u8; HEADER_LEN];
        self.stream
            .read_exact(&mut header)
            .await
            .map_err(|e| m_error!(EC::NetErr, "read response header error", e))?;
        let payload_len =
            u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
        let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
        frame_bytes.extend_from_slice(&header);
        if payload_len > 0 {
            let mut body = vec![0u8; payload_len];
            self.stream
                .read_exact(&mut body)
                .await
                .map_err(|e| m_error!(EC::NetErr, "read response payload error", e))?;
            frame_bytes.extend_from_slice(&body);
        }
        let frame = Frame::decode(&frame_bytes)?;
        self.ensure_success_frame(&frame)?;
        Ok(frame)
    }

    fn ensure_success_frame(&self, frame: &Frame) -> RS<()> {
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(frame)?;
            return Err(m_error!(EC::NetErr, error.message()));
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl AsyncClient for AsyncClientImpl {
    async fn query(&mut self, request: ClientRequest) -> RS<ServerResponse> {
        let payload = encode_client_request_with_message_type(
            MessageType::Query,
            self.take_request_id(),
            &request,
        )?;
        let frame = self.send_and_receive(&payload).await?;
        decode_server_response(&frame)
    }

    async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse> {
        let payload = encode_client_request_with_message_type(
            MessageType::Execute,
            self.take_request_id(),
            &request,
        )?;
        let frame = self.send_and_receive(&payload).await?;
        decode_server_response(&frame)
    }

    async fn get(&mut self, request: GetRequest) -> RS<GetResponse> {
        let payload = encode_get_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_get_response(&frame)
    }

    async fn put(&mut self, request: PutRequest) -> RS<PutResponse> {
        let payload = encode_put_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_put_response(&frame)
    }

    async fn range_scan(&mut self, request: RangeScanRequest) -> RS<RangeScanResponse> {
        let payload = encode_range_scan_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_range_scan_response(&frame)
    }

    async fn invoke_procedure(
        &mut self,
        request: ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        let payload = encode_procedure_invoke_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_procedure_invoke_response(&frame)
    }

    async fn create_session(&mut self, request: SessionCreateRequest) -> RS<SessionCreateResponse> {
        let payload = encode_session_create_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_session_create_response(&frame)
    }

    async fn close_session(&mut self, request: SessionCloseRequest) -> RS<SessionCloseResponse> {
        let payload = encode_session_close_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload).await?;
        decode_session_close_response(&frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::protocol::{
        GetResponse, KeyValue, PutResponse, SessionCloseResponse, SessionCreateResponse,
        decode_client_request, decode_get_request, decode_procedure_invoke_request,
        decode_put_request, decode_range_scan_request, decode_session_close_request,
        decode_session_create_request, encode_get_response, encode_procedure_invoke_response,
        encode_put_response, encode_range_scan_response, encode_server_response,
        encode_session_close_response, encode_session_create_response,
    };
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[tokio::test]
    async fn tokio_client_supports_query_and_execute() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();

            let query_frame = read_frame(&mut socket);
            assert_eq!(query_frame.header().message_type(), MessageType::Query);
            let query = decode_client_request(&query_frame).unwrap();
            assert_eq!(query.app_name(), "demo");
            assert_eq!(query.sql(), "select 1");
            socket
                .write_all(
                    &encode_server_response(
                        query_frame.header().request_id(),
                        &ServerResponse::new(
                            vec!["value".to_string()],
                            vec![vec!["1".to_string()]],
                            0,
                            None,
                        ),
                    )
                    .unwrap(),
                )
                .unwrap();

            let execute_frame = read_frame(&mut socket);
            assert_eq!(execute_frame.header().message_type(), MessageType::Execute);
            let execute = decode_client_request(&execute_frame).unwrap();
            assert_eq!(execute.sql(), "delete from t");
            socket
                .write_all(
                    &encode_server_response(
                        execute_frame.header().request_id(),
                        &ServerResponse::new(vec![], vec![], 2, None),
                    )
                    .unwrap(),
                )
                .unwrap();
        });

        let mut client = AsyncClientImpl::connect(&addr.to_string()).await.unwrap();
        let query = client
            .query(ClientRequest::new("demo", "select 1"))
            .await
            .unwrap();
        assert_eq!(query.columns(), &["value".to_string()]);
        assert_eq!(query.rows(), &[vec!["1".to_string()]]);

        let execute = client
            .execute(ClientRequest::new("demo", "delete from t"))
            .await
            .unwrap();
        assert_eq!(execute.affected_rows(), 2);

        server.join().unwrap();
    }

    #[tokio::test]
    async fn tokio_client_supports_kv_and_invoke_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();

            let create_frame = read_frame(&mut socket);
            let create_request = decode_session_create_request(&create_frame).unwrap();
            assert_eq!(create_request.config_json(), Some("{\"partition\":1}"));
            socket
                .write_all(
                    &encode_session_create_response(
                        create_frame.header().request_id(),
                        &SessionCreateResponse::new(88),
                    )
                    .unwrap(),
                )
                .unwrap();

            let put_frame = read_frame(&mut socket);
            let put_request = decode_put_request(&put_frame).unwrap();
            assert_eq!(put_request.session_id(), 88);
            assert_eq!(put_request.key(), b"key");
            assert_eq!(put_request.value(), b"value");
            socket
                .write_all(
                    &encode_put_response(put_frame.header().request_id(), &PutResponse::new(true))
                        .unwrap(),
                )
                .unwrap();

            let get_frame = read_frame(&mut socket);
            let get_request = decode_get_request(&get_frame).unwrap();
            assert_eq!(get_request.session_id(), 88);
            assert_eq!(get_request.key(), b"key");
            socket
                .write_all(
                    &encode_get_response(
                        get_frame.header().request_id(),
                        &GetResponse::new(Some(b"value".to_vec())),
                    )
                    .unwrap(),
                )
                .unwrap();

            let range_frame = read_frame(&mut socket);
            let range_request = decode_range_scan_request(&range_frame).unwrap();
            assert_eq!(range_request.start_key(), b"a");
            assert_eq!(range_request.end_key(), b"z");
            socket
                .write_all(
                    &encode_range_scan_response(
                        range_frame.header().request_id(),
                        &RangeScanResponse::new(vec![KeyValue::new(b"a".to_vec(), b"1".to_vec())]),
                    )
                    .unwrap(),
                )
                .unwrap();

            let invoke_frame = read_frame(&mut socket);
            let invoke_request = decode_procedure_invoke_request(&invoke_frame).unwrap();
            assert_eq!(invoke_request.session_id(), 88);
            assert_eq!(invoke_request.procedure_name(), "app/mod/proc");
            assert_eq!(invoke_request.procedure_parameters(), b"payload");
            socket
                .write_all(
                    &encode_procedure_invoke_response(
                        invoke_frame.header().request_id(),
                        &ProcedureInvokeResponse::new(br#"{"ok":true}"#.to_vec()),
                    )
                    .unwrap(),
                )
                .unwrap();

            let close_frame = read_frame(&mut socket);
            let close_request = decode_session_close_request(&close_frame).unwrap();
            assert_eq!(close_request.session_id(), 88);
            socket
                .write_all(
                    &encode_session_close_response(
                        close_frame.header().request_id(),
                        &SessionCloseResponse::new(true),
                    )
                    .unwrap(),
                )
                .unwrap();
        });

        let mut client = AsyncClientImpl::connect(&addr.to_string()).await.unwrap();
        let create = client
            .create_session(SessionCreateRequest::new(Some(
                "{\"partition\":1}".to_string(),
            )))
            .await
            .unwrap();
        assert_eq!(create.session_id(), 88);

        let put = client
            .put(PutRequest::new(88, b"key".to_vec(), b"value".to_vec()))
            .await
            .unwrap();
        assert!(put.ok());

        let get = client
            .get(GetRequest::new(88, b"key".to_vec()))
            .await
            .unwrap();
        assert_eq!(get.into_value(), Some(b"value".to_vec()));

        let range = client
            .range_scan(RangeScanRequest::new(88, b"a".to_vec(), b"z".to_vec()))
            .await
            .unwrap();
        assert_eq!(
            range.into_items(),
            vec![KeyValue::new(b"a".to_vec(), b"1".to_vec())]
        );

        let invoke = client
            .invoke_procedure(ProcedureInvokeRequest::new(
                88,
                "app/mod/proc",
                b"payload".to_vec(),
            ))
            .await
            .unwrap();
        assert_eq!(invoke.into_result(), br#"{"ok":true}"#.to_vec());

        let close = client
            .close_session(SessionCloseRequest::new(88))
            .await
            .unwrap();
        assert!(close.closed());

        server.join().unwrap();
    }

    fn read_frame(socket: &mut std::net::TcpStream) -> Frame {
        let mut header = [0u8; HEADER_LEN];
        socket.read_exact(&mut header).unwrap();
        let payload_len =
            u32::from_be_bytes([header[16], header[17], header[18], header[19]]) as usize;
        let mut body = vec![0u8; payload_len];
        if payload_len > 0 {
            socket.read_exact(&mut body).unwrap();
        }
        let mut frame = Vec::from(header);
        frame.extend_from_slice(&body);
        Frame::decode(&frame).unwrap()
    }
}
