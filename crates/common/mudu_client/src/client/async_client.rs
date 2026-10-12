//! Async io_uring TCP client for the MuduDB wire protocol.

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    ClientRequest, Frame, FrameHeader, GetRequest, GetResponse, HEADER_LEN, MessageType,
    ProcedureInvokeRequest, ProcedureInvokeResponse, PutRequest, PutResponse, RangeScanRequest,
    RangeScanResponse, ServerPerfDigest, ServerResponse, SessionCloseRequest, SessionCloseResponse,
    SessionCreateRequest, SessionCreateResponse, decode_error_response, decode_get_response,
    decode_procedure_invoke_response, decode_put_response, decode_range_scan_response,
    decode_server_response, decode_session_close_response, decode_session_create_response,
    encode_batch_request, encode_client_request_with_message_type,
    encode_client_request_with_message_type_and_trace, encode_get_request,
    encode_procedure_invoke_request_with_trace, encode_put_request, encode_range_scan_request,
    encode_session_close_request, encode_session_create_request,
};
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::perf::{PerfSpan, TraceContext, TxnStage, next_trace_id, should_sample};
use mudu_sys::time::instant_now;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Async interface for sending MuduDB protocol requests over TCP.
#[async_trait]
pub trait AsyncClient: Send {
    /// Send a SQL query request.
    async fn query(&mut self, request: ClientRequest) -> RS<ServerResponse>;
    /// Send a SQL execute request.
    async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse>;
    /// Send a batched request.
    async fn batch(&mut self, request: ClientRequest) -> RS<ServerResponse>;
    /// Send a KV get request.
    async fn get(&mut self, request: GetRequest) -> RS<GetResponse>;
    /// Send a KV put request.
    async fn put(&mut self, request: PutRequest) -> RS<PutResponse>;
    /// Send a KV range scan request.
    async fn range_scan(&mut self, request: RangeScanRequest) -> RS<RangeScanResponse>;
    /// Invoke a stored procedure.
    async fn invoke_procedure(
        &mut self,
        request: ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse>;
    /// Create a new session.
    async fn create_session(&mut self, request: SessionCreateRequest) -> RS<SessionCreateResponse>;
    /// Close an existing session.
    async fn close_session(&mut self, request: SessionCloseRequest) -> RS<SessionCloseResponse>;
}

/// Async TCP client implementation using io_uring.
pub struct AsyncClientImpl {
    stream: AsyncTcpStream,
    addr: String,
    next_request_id: u64,
}

impl AsyncClientImpl {
    /// Connect to `addr` and return a new client.
    pub async fn connect(addr: &str) -> RS<Self> {
        let stream = Self::connect_stream(addr).await?;
        Ok(Self {
            stream,
            addr: addr.to_string(),
            next_request_id: 1,
        })
    }

    async fn connect_stream(addr: &str) -> RS<AsyncTcpStream> {
        let stream = AsyncTcpStream::connect(addr).await.map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("connect io_uring tcp server error: addr={addr}"),
                e
            )
        })?;
        stream.set_nodelay(true).map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("set tcp nodelay error: addr={addr}"),
                e
            )
        })?;
        Ok(stream)
    }

    fn take_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        request_id
    }

    async fn send_and_receive(&mut self, payload: &[u8], trace_id: u64) -> RS<Frame> {
        let frame = self.roundtrip(payload, trace_id).await?;
        // A leader hint means the peer is not the serving raft leader:
        // reconnect to the hinted leader and replay the request once. Note
        // that session-bound requests may need a fresh session on the new
        // leader (sessions are server-local); the caller sees the session
        // error and can act on it.
        if frame.header().message_type() == MessageType::Error {
            let hinted_addr = decode_error_response(&frame)
                .ok()
                .and_then(|error| {
                    error
                        .leader_hint()
                        .map(|hint| (hint.host().to_string(), hint.port()))
                })
                .map(|(host, port)| format!("{host}:{port}"));
            if let Some(addr) = hinted_addr
                && addr != self.addr
            {
                self.stream = Self::connect_stream(&addr).await?;
                self.addr = addr;
                let frame = self.roundtrip(payload, trace_id).await?;
                self.ensure_success_frame(&frame)?;
                return Ok(frame);
            }
        }
        self.ensure_success_frame(&frame)?;
        Ok(frame)
    }

    async fn roundtrip(&mut self, payload: &[u8], trace_id: u64) -> RS<Frame> {
        {
            let _send = PerfSpan::new(TxnStage::ClientNetworkSend, trace_id);
            self.stream
                .write_all(payload)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, "write request frame error", e))?;
            self.stream
                .flush()
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, "flush request frame error", e))?;
        }

        let frame = {
            let _recv = PerfSpan::new(TxnStage::ClientNetworkRecv, trace_id);
            let mut header = [0u8; HEADER_LEN];
            self.stream
                .read_exact(&mut header)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, "read response header error", e))?;
            let payload_len = FrameHeader::decode_header_bytes(&header)?.payload_len() as usize;
            let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
            frame_bytes.extend_from_slice(&header);
            if payload_len > 0 {
                let mut body = vec![0u8; payload_len];
                self.stream.read_exact(&mut body).await.map_err(|e| {
                    mudu_error!(ErrorCode::Network, "read response payload error", e)
                })?;
                frame_bytes.extend_from_slice(&body);
            }
            Frame::decode(&frame_bytes)?
        };
        Ok(frame)
    }

    fn ensure_success_frame(&self, frame: &Frame) -> RS<()> {
        if frame.header().message_type() == MessageType::Error {
            let error = decode_error_response(frame)?;
            let ec = mudu::error::ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal);
            let msg = if error.name().is_empty() {
                error.message().to_string()
            } else {
                format!("{}({}): {}", error.name(), error.code(), error.message())
            };
            return Err(mudu_error!(ec, msg));
        }
        Ok(())
    }

    fn log_end_to_end_perf(
        trace_id: u64,
        total_ns: u64,
        serialize_ns: u64,
        deserialize_ns: u64,
        server_exec_stage: TxnStage,
        server_digest: &ServerPerfDigest,
    ) {
        let net_recv = server_digest.get(TxnStage::NetworkRecv).unwrap_or(0);
        let server_exec = server_digest.get(server_exec_stage).unwrap_or(0);
        let network_rtt = total_ns.saturating_sub(serialize_ns + server_exec + deserialize_ns);
        tracing::info!(
            trace_id,
            total_us = total_ns / 1000,
            serialize_us = serialize_ns / 1000,
            network_rtt_us = network_rtt / 1000,
            server_network_recv_us = net_recv / 1000,
            server_exec_us = server_exec / 1000,
            server_exec_stage = ?server_exec_stage,
            deserialize_us = deserialize_ns / 1000,
            "end-to-end perf",
        );
    }
}

#[async_trait]
impl AsyncClient for AsyncClientImpl {
    async fn query(&mut self, request: ClientRequest) -> RS<ServerResponse> {
        let trace_id = if should_sample() { next_trace_id() } else { 0 };
        let _total = PerfSpan::new(TxnStage::Total, trace_id);
        let request_id = self.take_request_id();
        let trace_context = if trace_id != 0 {
            TraceContext::new(trace_id)
        } else {
            TraceContext::empty()
        };

        let total_start = instant_now();

        let payload = {
            let _s = PerfSpan::new(TxnStage::ClientSerialize, trace_id);
            let start = instant_now();
            let payload = encode_client_request_with_message_type_and_trace(
                MessageType::Query,
                request_id,
                trace_context,
                &request,
            )?;
            (payload, start.elapsed().as_nanos() as u64)
        };
        let (payload, serialize_ns) = payload;

        let frame = {
            let _s = PerfSpan::new(TxnStage::ClientNetworkSend, trace_id);
            let _s = PerfSpan::new(TxnStage::ClientNetworkRecv, trace_id);
            self.send_and_receive(&payload, trace_id).await?
        };

        let response = {
            let _s = PerfSpan::new(TxnStage::ClientDeserialize, trace_id);
            let start = instant_now();
            let response = decode_server_response(&frame)?;
            (response, start.elapsed().as_nanos() as u64)
        };
        let (response, deserialize_ns) = response;
        let total_ns = total_start.elapsed().as_nanos() as u64;

        if trace_id != 0
            && let Some(server_digest) = response.server_perf_digest()
        {
            Self::log_end_to_end_perf(
                trace_id,
                total_ns,
                serialize_ns,
                deserialize_ns,
                TxnStage::QueryExec,
                server_digest,
            );
        }

        Ok(response)
    }

    async fn execute(&mut self, request: ClientRequest) -> RS<ServerResponse> {
        let payload = encode_client_request_with_message_type(
            MessageType::Execute,
            self.take_request_id(),
            &request,
        )?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_server_response(&frame)
    }

    async fn batch(&mut self, request: ClientRequest) -> RS<ServerResponse> {
        let payload = encode_batch_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_server_response(&frame)
    }

    async fn get(&mut self, request: GetRequest) -> RS<GetResponse> {
        let payload = encode_get_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_get_response(&frame)
    }

    async fn put(&mut self, request: PutRequest) -> RS<PutResponse> {
        let payload = encode_put_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_put_response(&frame)
    }

    async fn range_scan(&mut self, request: RangeScanRequest) -> RS<RangeScanResponse> {
        let payload = encode_range_scan_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_range_scan_response(&frame)
    }

    async fn invoke_procedure(
        &mut self,
        request: ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        let trace_id = if should_sample() { next_trace_id() } else { 0 };
        let _total = PerfSpan::new(TxnStage::Total, trace_id);
        let request_id = self.take_request_id();
        let trace_context = if trace_id != 0 {
            TraceContext::new(trace_id)
        } else {
            TraceContext::empty()
        };

        let total_start = instant_now();

        let payload = {
            let _s = PerfSpan::new(TxnStage::ClientSerialize, trace_id);
            let start = instant_now();
            let payload =
                encode_procedure_invoke_request_with_trace(request_id, trace_context, &request)?;
            (payload, start.elapsed().as_nanos() as u64)
        };
        let (payload, serialize_ns) = payload;

        let frame = {
            let _s = PerfSpan::new(TxnStage::ClientNetworkSend, trace_id);
            let _s = PerfSpan::new(TxnStage::ClientNetworkRecv, trace_id);
            self.send_and_receive(&payload, trace_id).await?
        };

        let response = {
            let _s = PerfSpan::new(TxnStage::ClientDeserialize, trace_id);
            let start = instant_now();
            let response = decode_procedure_invoke_response(&frame)?;
            (response, start.elapsed().as_nanos() as u64)
        };
        let (response, deserialize_ns) = response;
        let total_ns = total_start.elapsed().as_nanos() as u64;

        if trace_id != 0
            && let Some(server_digest) = response.server_perf_digest()
        {
            Self::log_end_to_end_perf(
                trace_id,
                total_ns,
                serialize_ns,
                deserialize_ns,
                TxnStage::ProcedureExec,
                server_digest,
            );
        }

        Ok(response)
    }

    async fn create_session(&mut self, request: SessionCreateRequest) -> RS<SessionCreateResponse> {
        let payload = encode_session_create_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
        decode_session_create_response(&frame)
    }

    async fn close_session(&mut self, request: SessionCloseRequest) -> RS<SessionCloseResponse> {
        let payload = encode_session_close_request(self.take_request_id(), &request)?;
        let frame = self.send_and_receive(&payload, 0).await?;
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
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_sys::net::sync::StdTcpListener;
    use mudu_sys::task::sync::spawn_thread;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::type_family::TypeFamily;
    use std::io::{Read, Write};

    fn bind_test_listener() -> Option<StdTcpListener> {
        match StdTcpListener::bind("127.0.0.1:0".parse().unwrap()) {
            Ok(listener) => Some(listener),
            Err(err) => {
                eprintln!("skip async tcp client test: {err}");
                None
            }
        }
    }

    #[test]
    fn tokio_client_supports_query_and_execute() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let Some(listener) = bind_test_listener() else {
                return;
            };
            let addr = listener.local_addr().unwrap();
            let server = spawn_thread(move || {
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
                                TupleFieldDesc::new(vec![DatumDesc::new(
                                    "value".to_string(),
                                    DataType::default_for(TypeFamily::String),
                                )]),
                                vec![TupleValue::from(vec![DataValue::from_string(
                                    "1".to_string(),
                                )])],
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
                            &ServerResponse::new(TupleFieldDesc::new(vec![]), vec![], 2, None),
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
            assert_eq!(query.row_desc().fields()[0].name(), "value");
            assert_eq!(query.rows()[0].values()[0].expect_string(), "1");

            let execute = client
                .execute(ClientRequest::new("demo", "delete from t"))
                .await
                .unwrap();
            assert_eq!(execute.affected_rows(), 2);

            server.unwrap().join().unwrap();
        })
        .unwrap();
    }

    fn server_exchange<Req, Resp>(
        socket: &mut mudu_sys::net::sync::SStdTcpStream,
        decode: fn(&Frame) -> RS<Req>,
        check: impl FnOnce(&Req),
        response: Resp,
        encode: fn(u64, &Resp) -> RS<Vec<u8>>,
    ) {
        let frame = read_frame(socket);
        let request = decode(&frame).unwrap();
        check(&request);
        socket
            .write_all(&encode(frame.header().request_id(), &response).unwrap())
            .unwrap();
    }

    fn run_kv_invoke_server(listener: StdTcpListener) {
        let (mut socket, _) = listener.accept().unwrap();

        server_exchange(
            &mut socket,
            decode_session_create_request,
            |create| assert_eq!(create.config_json(), Some("{\"worker_id\":1}")),
            SessionCreateResponse::new(88),
            encode_session_create_response,
        );

        server_exchange(
            &mut socket,
            decode_put_request,
            |put| {
                assert_eq!(put.session_id(), 88);
                assert_eq!(put.key(), b"key");
                assert_eq!(put.value(), b"value");
            },
            PutResponse::new(true),
            encode_put_response,
        );

        server_exchange(
            &mut socket,
            decode_get_request,
            |get| {
                assert_eq!(get.session_id(), 88);
                assert_eq!(get.key(), b"key");
            },
            GetResponse::new(Some(b"value".to_vec())),
            encode_get_response,
        );

        server_exchange(
            &mut socket,
            decode_range_scan_request,
            |range| {
                assert_eq!(range.start_key(), b"a");
                assert_eq!(range.end_key(), b"z");
            },
            RangeScanResponse::new(vec![KeyValue::new(b"a".to_vec(), b"1".to_vec())]),
            encode_range_scan_response,
        );

        server_exchange(
            &mut socket,
            decode_procedure_invoke_request,
            |invoke| {
                assert_eq!(invoke.session_id(), 88);
                assert_eq!(invoke.procedure_name(), "app/mod/proc");
                assert_eq!(invoke.procedure_parameters(), b"payload");
            },
            ProcedureInvokeResponse::new(br#"{"ok":true}"#.to_vec()),
            encode_procedure_invoke_response,
        );

        server_exchange(
            &mut socket,
            decode_session_close_request,
            |close| assert_eq!(close.session_id(), 88),
            SessionCloseResponse::new(true),
            encode_session_close_response,
        );
    }

    async fn run_kv_invoke_client(addr: &str) {
        let mut client = AsyncClientImpl::connect(addr).await.unwrap();
        let create = client
            .create_session(SessionCreateRequest::new(Some(
                "{\"worker_id\":1}".to_string(),
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
    }

    #[test]
    fn tokio_client_supports_kv_and_invoke_roundtrip() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let Some(listener) = bind_test_listener() else {
                return;
            };
            let addr = listener.local_addr().unwrap();
            let server = spawn_thread(move || run_kv_invoke_server(listener));
            run_kv_invoke_client(&addr.to_string()).await;
            server.unwrap().join().unwrap();
        })
        .unwrap();
    }

    fn read_frame(socket: &mut mudu_sys::net::sync::SStdTcpStream) -> Frame {
        let mut header = [0u8; HEADER_LEN];
        socket.read_exact(&mut header).unwrap();
        let payload_len = FrameHeader::decode_header_bytes(&header)
            .unwrap()
            .payload_len() as usize;
        let mut body = vec![0u8; payload_len];
        if payload_len > 0 {
            socket.read_exact(&mut body).unwrap();
        }
        let mut frame = Vec::from(header);
        frame.extend_from_slice(&body);
        Frame::decode(&frame).unwrap()
    }
}
