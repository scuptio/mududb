//! Tests for the protocol module.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(test)]
mod tests {
    use super::super::*;
    use mudu::common::result::RS;
    use mudu_sys_contract::perf::{TraceContext, TxnStage};

    #[test]
    fn frame_roundtrip_preserves_header_and_payload() {
        let frame = Frame::new(MessageType::ProcedureInvoke, 42, b"payload".to_vec());
        let encoded = frame.encode();
        let decoded = Frame::decode(&encoded).unwrap();

        assert_eq!(decoded.header().magic(), 0x4D53_464D);
        assert_eq!(decoded.header().version(), 1);
        assert_eq!(
            decoded.header().message_type(),
            MessageType::ProcedureInvoke
        );
        assert_eq!(decoded.header().request_id(), 42);
        assert_eq!(decoded.payload(), b"payload");
    }

    #[test]
    fn frame_decode_rejects_bad_magic_and_incomplete_payload() {
        let mut encoded = Frame::new(MessageType::Get, 7, vec![1, 2, 3]).encode();
        encoded[0] = 0;
        let bad_magic = Frame::decode(&encoded).unwrap_err();
        assert!(format!("{bad_magic}").contains("invalid protocol frame magic"));

        let encoded = Frame::new(MessageType::Put, 8, vec![1, 2, 3, 4]).encode();
        let truncated = &encoded[..encoded.len() - 2];
        let incomplete = Frame::decode(truncated).unwrap_err();
        assert!(format!("{incomplete}").contains("frame payload is incomplete"));
    }

    #[test]
    fn query_and_execute_requests_roundtrip() {
        let request = ClientRequest::new("demo", "select 1");
        let query = encode_client_request(1, &request).unwrap();
        let query_frame = Frame::decode(&query).unwrap();
        assert_eq!(query_frame.header().message_type(), MessageType::Query);
        let query_decoded = decode_client_request(&query_frame).unwrap();
        assert_eq!(query_decoded.app_name(), "demo");
        assert_eq!(query_decoded.sql(), "select 1");

        let execute =
            encode_client_request_with_message_type(MessageType::Execute, 2, &request).unwrap();
        let execute_frame = Frame::decode(&execute).unwrap();
        assert_eq!(execute_frame.header().message_type(), MessageType::Execute);
        let execute_decoded = decode_client_request(&execute_frame).unwrap();
        assert_eq!(execute_decoded.app_name(), "demo");
        assert_eq!(execute_decoded.sql(), "select 1");
    }

    #[test]
    fn kv_and_session_messages_roundtrip() {
        let get_frame =
            Frame::decode(&encode_get_request(1, &GetRequest::new(9, b"key".to_vec())).unwrap())
                .unwrap();
        let get_request = decode_get_request(&get_frame).unwrap();
        assert_eq!(get_request.session_id(), 9);
        assert_eq!(get_request.key(), b"key");

        let put_frame = Frame::decode(
            &encode_put_request(2, &PutRequest::new(9, b"k".to_vec(), b"v".to_vec())).unwrap(),
        )
        .unwrap();
        let put_request = decode_put_request(&put_frame).unwrap();
        assert_eq!(put_request.session_id(), 9);
        assert_eq!(put_request.key(), b"k");
        assert_eq!(put_request.value(), b"v");
        assert_eq!(put_request.into_parts(), (b"k".to_vec(), b"v".to_vec()));

        let range_frame = Frame::decode(
            &encode_range_scan_request(3, &RangeScanRequest::new(9, b"a".to_vec(), b"z".to_vec()))
                .unwrap(),
        )
        .unwrap();
        let range_request = decode_range_scan_request(&range_frame).unwrap();
        assert_eq!(range_request.start_key(), b"a");
        assert_eq!(range_request.end_key(), b"z");

        let create_frame = Frame::decode(
            &encode_session_create_request(
                4,
                &SessionCreateRequest::new(Some("{\"partition\":1}".to_string())),
            )
            .unwrap(),
        )
        .unwrap();
        let create_request = decode_session_create_request(&create_frame).unwrap();
        assert_eq!(create_request.config_json(), Some("{\"partition\":1}"));

        let empty_create_frame = Frame::new(MessageType::SessionCreate, 5, vec![]);
        let empty_create_request = decode_session_create_request(&empty_create_frame).unwrap();
        assert_eq!(empty_create_request.config_json(), None);

        let close_frame =
            Frame::decode(&encode_session_close_request(6, &SessionCloseRequest::new(9)).unwrap())
                .unwrap();
        let close_request = decode_session_close_request(&close_frame).unwrap();
        assert_eq!(close_request.session_id(), 9);
    }

    #[test]
    fn invoke_and_response_messages_roundtrip() {
        let invoke_frame = Frame::decode(
            &encode_procedure_invoke_request(
                10,
                &ProcedureInvokeRequest::new(11, "app/mod/proc", b"input".to_vec()),
            )
            .unwrap(),
        )
        .unwrap();
        let invoke_request = decode_procedure_invoke_request(&invoke_frame).unwrap();
        assert_eq!(invoke_request.session_id(), 11);
        assert_eq!(invoke_request.procedure_name(), "app/mod/proc");
        assert_eq!(invoke_request.procedure_parameters(), b"input");
        assert_eq!(
            invoke_request.procedure_parameters_owned(),
            b"input".to_vec()
        );

        use crate::tuple::datum_desc::DatumDesc;
        use crate::tuple::tuple_field_desc::TupleFieldDesc;
        use crate::tuple::tuple_value::TupleValue;
        use mudu_type::data_type::DataType;
        use mudu_type::data_value::DataValue;
        use mudu_type::type_family::TypeFamily;

        let response = ServerResponse::new(
            TupleFieldDesc::new(vec![DatumDesc::new(
                "value".to_string(),
                DataType::default_for(TypeFamily::String),
            )]),
            vec![TupleValue::from(vec![DataValue::from_string(
                "1".to_string(),
            )])],
            0,
            None,
        );
        let response_frame =
            Frame::decode(&encode_server_response(12, &response).unwrap()).unwrap();
        let decoded_response = decode_server_response(&response_frame).unwrap();
        assert_eq!(decoded_response.row_desc().fields()[0].name(), "value");
        assert_eq!(decoded_response.rows()[0].values()[0].expect_string(), "1");

        let get_response_frame = Frame::decode(
            &encode_get_response(13, &GetResponse::new(Some(b"v".to_vec()))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_get_response(&get_response_frame)
                .unwrap()
                .into_value(),
            Some(b"v".to_vec())
        );

        let range_response_frame = Frame::decode(
            &encode_range_scan_response(
                14,
                &RangeScanResponse::new(vec![KeyValue::new(b"k".to_vec(), b"v".to_vec())]),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_range_scan_response(&range_response_frame)
                .unwrap()
                .into_items(),
            vec![KeyValue::new(b"k".to_vec(), b"v".to_vec())]
        );

        let invoke_response_frame = Frame::decode(
            &encode_procedure_invoke_response(15, &ProcedureInvokeResponse::new(b"ok".to_vec()))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            decode_procedure_invoke_response(&invoke_response_frame)
                .unwrap()
                .into_result(),
            b"ok".to_vec()
        );
    }

    #[test]
    fn error_response_roundtrip() {
        let frame = Frame::decode(&encode_error_response(99, "boom").unwrap()).unwrap();
        assert_eq!(frame.header().message_type(), MessageType::Error);
        let error = decode_error_response(&frame).unwrap();
        assert_eq!(error.message(), "boom");
        assert_eq!(error.name(), "Internal");
        assert_eq!(error.code(), ErrorCode::Internal.to_u32());
    }

    #[test]
    fn merror_response_roundtrip() {
        let err = mudu_error!(ErrorCode::Parse, "bad request");
        let frame = Frame::decode(&encode_merror_response(42, &err).unwrap()).unwrap();
        let error = decode_error_response(&frame).unwrap();
        assert_eq!(error.message(), "bad request");
        assert_eq!(error.name(), "Parse");
        assert_eq!(error.code(), ErrorCode::Parse.to_u32());
    }

    #[test]
    fn message_type_try_from_covers_all_values_and_errors() {
        let cases = [
            (1u32, MessageType::Handshake),
            (2, MessageType::Auth),
            (3, MessageType::Query),
            (4, MessageType::Execute),
            (5, MessageType::Batch),
            (6, MessageType::Response),
            (7, MessageType::Error),
            (8, MessageType::Get),
            (9, MessageType::Put),
            (10, MessageType::RangeScan),
            (11, MessageType::ProcedureInvoke),
            (12, MessageType::SessionCreate),
            (13, MessageType::SessionClose),
            (14, MessageType::ReplSubscribe),
            (15, MessageType::ReplBatch),
            (16, MessageType::ReplAck),
            (17, MessageType::ReplHealFetch),
            (18, MessageType::ReplHealResp),
            (19, MessageType::RaftPreVoteReq),
            (20, MessageType::RaftPreVoteResp),
            (21, MessageType::RaftVoteReq),
            (22, MessageType::RaftVoteResp),
            (23, MessageType::RaftAppendReq),
            (24, MessageType::RaftAppendResp),
            (25, MessageType::RaftApplyReq),
            (26, MessageType::RaftApplyResp),
            (27, MessageType::RaftUpdateConfReq),
            (28, MessageType::RaftUpdateConfResp),
            (29, MessageType::ReplSnapshotOffer),
            (30, MessageType::ReplSnapshotFetch),
            (31, MessageType::ReplSnapshotResp),
            (32, MessageType::ReplSubscribeAccept),
        ];
        for (value, expected) in cases {
            assert_eq!(MessageType::try_from(value).unwrap(), expected);
            assert_eq!(u32::from(expected), value);
        }
        assert!(MessageType::try_from(0).is_err());
        assert!(MessageType::try_from(33).is_err());
    }

    #[test]
    fn server_perf_digest_accessors() {
        let mut digest = ServerPerfDigest::new(42);
        assert_eq!(digest.trace_id, 42);
        assert_eq!(digest.get(TxnStage::Parse), None);
        digest.set(TxnStage::Parse, 123);
        assert_eq!(digest.get(TxnStage::Parse), Some(123));
    }

    #[test]
    fn frame_header_and_accessors() {
        let header = FrameHeader::new(MessageType::Query, 10, 5);
        assert_eq!(header.magic(), format::latest::MAGIC);
        assert_eq!(header.version(), format::latest::FRAME_VERSION);
        assert_eq!(header.message_type(), MessageType::Query);
        assert_eq!(header.flags(), 0);
        assert!(!header.sampled());
        assert_eq!(header.request_id(), 10);
        assert_eq!(header.trace_context(), TraceContext::empty());
        assert_eq!(header.payload_len(), 5);

        let ctx = TraceContext::new(7);
        let sampled_header = FrameHeader::new_with_trace(MessageType::Put, 11, ctx, 6);
        assert_eq!(sampled_header.flags(), format::latest::FLAG_SAMPLED);
        assert!(sampled_header.sampled());
        assert_eq!(sampled_header.trace_context(), ctx);

        let frame = Frame::new_with_trace(MessageType::Response, 12, ctx, b"body".to_vec());
        let encoded = frame.encode();
        let decoded_header =
            FrameHeader::decode_header_bytes(&encoded[..format::latest::HEADER_LEN]).unwrap();
        assert_eq!(decoded_header.message_type(), MessageType::Response);
        assert_eq!(decoded_header.request_id(), 12);
        assert!(decoded_header.sampled());
    }

    #[test]
    fn frame_from_parts_rejects_mismatch_and_into_payload_works() {
        let header = FrameHeader::new(MessageType::Query, 1, 10);
        let err = Frame::from_parts(header, vec![1, 2]).unwrap_err();
        assert!(err.to_string().contains("payload length mismatch"));

        let frame = Frame::new(MessageType::Query, 3, b"payload".to_vec());
        assert_eq!(frame.into_payload(), b"payload".to_vec());
    }

    #[test]
    fn client_request_with_oid_roundtrip() {
        let request = ClientRequest::new_with_oid(123, "app", "select 1");
        assert_eq!(request.oid(), 123);
        assert_eq!(request.app_name(), "app");
        assert_eq!(request.sql(), "select 1");
        assert!(request.params().is_empty());

        let encoded = encode_client_request_with_message_type_and_trace(
            MessageType::Query,
            1,
            TraceContext::new(9),
            &request,
        )
        .unwrap();
        let frame = Frame::decode(&encoded).unwrap();
        assert!(frame.header().sampled());
        let decoded = decode_client_request(&frame).unwrap();
        assert_eq!(decoded.oid(), 123);
    }

    #[test]
    fn client_request_params_roundtrip() {
        use mudu_type::data_value::DataValue;

        let params = vec![
            DataValue::from_i32(7),
            DataValue::from_i64(1_000_000_000),
            DataValue::from_string("3.00".to_string()),
            DataValue::from_f64(2.5),
        ];
        let request =
            ClientRequest::new_with_oid(5, "app", "update t set bal = bal - ? where id = ?;")
                .with_params(params.clone());

        for message_type in [MessageType::Query, MessageType::Execute, MessageType::Batch] {
            let encoded =
                encode_client_request_with_message_type(message_type, 9, &request).unwrap();
            let frame = Frame::decode(&encoded).unwrap();
            assert_eq!(frame.header().message_type(), message_type);
            let decoded = decode_client_request(&frame).unwrap();
            assert_eq!(decoded.oid(), 5);
            assert_eq!(decoded.sql(), "update t set bal = bal - ? where id = ?;");
            assert_eq!(decoded.params().len(), params.len());
            assert_eq!(decoded.params()[0].expect_i32(), &7);
            assert_eq!(decoded.params()[1].expect_i64(), &1_000_000_000);
            assert_eq!(decoded.params()[2].expect_string(), "3.00");
            assert_eq!(decoded.params()[3].expect_f64(), &2.5);
        }
    }

    #[test]
    fn client_request_decodes_legacy_payload_without_params() {
        // Payloads encoded before the `params` field existed hold only
        // (oid, app_name, sql); the serde default must fill `params` with an
        // empty vector.
        let legacy_payload =
            rmp_serde::to_vec(&(7u128, "app".to_string(), "select 1".to_string())).unwrap();
        let frame = Frame::new(MessageType::Query, 1, legacy_payload);
        let decoded = decode_client_request(&frame).unwrap();
        assert_eq!(decoded.oid(), 7);
        assert_eq!(decoded.app_name(), "app");
        assert_eq!(decoded.sql(), "select 1");
        assert!(decoded.params().is_empty());
    }

    #[test]
    fn server_response_accessors_and_perf_digest() {
        use crate::tuple::datum_desc::DatumDesc;
        use crate::tuple::tuple_field_desc::TupleFieldDesc;
        use crate::tuple::tuple_value::TupleValue;
        use mudu_type::data_type::DataType;
        use mudu_type::data_value::DataValue;
        use mudu_type::type_family::TypeFamily;

        let row_desc = TupleFieldDesc::new(vec![DatumDesc::new(
            "c".to_string(),
            DataType::default_for(TypeFamily::String),
        )]);
        let rows = vec![TupleValue::from(vec![DataValue::from_string(
            "v".to_string(),
        )])];
        let response =
            ServerResponse::new(row_desc.clone(), rows.clone(), 3, Some("err".to_string()))
                .with_server_perf_digest(ServerPerfDigest::new(1));

        assert_eq!(response.row_desc().fields()[0].name(), "c");
        assert_eq!(response.rows()[0].values()[0].expect_string(), "v");
        assert_eq!(response.affected_rows(), 3);
        assert_eq!(response.error(), Some("err"));
        assert_eq!(response.server_perf_digest().unwrap().trace_id, 1);

        let encoded = encode_server_response(1, &response).unwrap();
        let decoded = decode_server_response(&Frame::decode(&encoded).unwrap()).unwrap();
        assert_eq!(decoded.server_perf_digest(), response.server_perf_digest());
    }

    #[test]
    fn handshake_roundtrip() {
        let request = HandshakeRequest {
            supported_versions: vec![1, 2],
            capabilities: vec!["cap".to_string()],
        };
        let encoded = encode_handshake_request(1, &request).unwrap();
        let decoded = decode_handshake_request(&Frame::decode(&encoded).unwrap()).unwrap();
        assert_eq!(decoded, request);

        let response = HandshakeResponse {
            selected_version: 1,
            capabilities: vec!["srv".to_string()],
        };
        let encoded = encode_handshake_response(2, &response).unwrap();
        let decoded = decode_handshake_response(&Frame::decode(&encoded).unwrap()).unwrap();
        assert_eq!(decoded, response);
    }

    #[test]
    fn batch_request_roundtrip() {
        let request = ClientRequest::new("app", "batch");
        let encoded = encode_batch_request(3, &request).unwrap();
        let frame = Frame::decode(&encoded).unwrap();
        assert_eq!(frame.header().message_type(), MessageType::Batch);
        assert_eq!(decode_client_request(&frame).unwrap().sql(), "batch");
    }

    #[test]
    fn put_and_session_response_roundtrips() {
        let put_resp = PutResponse::new(true);
        let encoded = encode_put_response(1, &put_resp).unwrap();
        let decoded = decode_put_response(&Frame::decode(&encoded).unwrap()).unwrap();
        assert!(decoded.ok());

        let create_resp = SessionCreateResponse::new(42);
        let encoded = encode_session_create_response(2, &create_resp).unwrap();
        let decoded = decode_session_create_response(&Frame::decode(&encoded).unwrap()).unwrap();
        assert_eq!(decoded.session_id(), 42);

        let close_resp = SessionCloseResponse::new(true);
        let encoded = encode_session_close_response(3, &close_resp).unwrap();
        let decoded = decode_session_close_response(&Frame::decode(&encoded).unwrap()).unwrap();
        assert!(decoded.closed());
    }

    #[test]
    fn request_and_response_getters() {
        let range = RangeScanRequest::new(7, b"a".to_vec(), b"z".to_vec());
        assert_eq!(range.session_id(), 7);

        let kv = KeyValue::new(b"k".to_vec(), b"v".to_vec());
        assert_eq!(kv.key(), b"k");
        assert_eq!(kv.value(), b"v");

        let get = GetResponse::new(Some(b"v".to_vec()));
        assert_eq!(get.value(), Some(b"v".as_slice()));
        assert_eq!(get.into_value(), Some(b"v".to_vec()));

        let put = PutResponse::new(false);
        assert!(!put.ok());

        let range_resp = RangeScanResponse::new(vec![KeyValue::new(b"k".to_vec(), b"v".to_vec())]);
        assert_eq!(range_resp.items().len(), 1);
        assert_eq!(range_resp.into_items().len(), 1);

        let invoke_resp = ProcedureInvokeResponse::new(b"ok".to_vec())
            .with_server_perf_digest(ServerPerfDigest::new(5));
        assert_eq!(invoke_resp.result(), b"ok");
        assert_eq!(invoke_resp.server_perf_digest().unwrap().trace_id, 5);
        assert_eq!(invoke_resp.into_result(), b"ok".to_vec());

        let create = SessionCreateRequest::new(None);
        assert_eq!(create.config_json(), None);

        let create_resp = SessionCreateResponse::new(99);
        assert_eq!(create_resp.session_id(), 99);

        let close_resp = SessionCloseResponse::new(false);
        assert!(!close_resp.closed());
    }

    #[test]
    fn procedure_invoke_with_trace_roundtrip() {
        let request = ProcedureInvokeRequest::new(1, "proc", b"input".to_vec());
        let encoded =
            encode_procedure_invoke_request_with_trace(2, TraceContext::new(5), &request).unwrap();
        let frame = Frame::decode(&encoded).unwrap();
        assert!(frame.header().sampled());
        assert_eq!(frame.header().trace_context().trace_id, 5);
        let decoded = decode_procedure_invoke_request(&frame).unwrap();
        assert_eq!(decoded.procedure_name(), "proc");
    }

    #[test]
    fn error_response_accessors() {
        let error = ErrorResponse::new("msg");
        assert_eq!(error.message(), "msg");
        assert_eq!(error.code(), ErrorCode::Internal.to_u32());
        assert_eq!(error.name(), "Internal");
        assert_eq!(error.source(), "");
        assert_eq!(error.location(), "");

        let err = mudu_error!(ErrorCode::Parse, "bad");
        let from_err = ErrorResponse::from_merror(&err);
        assert_eq!(from_err.source(), err.err_src().to_json_str());
        assert_eq!(from_err.location(), err.loc().to_string());
    }

    /// Encodes `message` with `encode`, decodes the resulting frame, asserts
    /// the frame carries `expected` as its message type, and decodes the
    /// payload back with `decode`.
    fn roundtrip_message<M>(
        request_id: u64,
        message: &M,
        expected: MessageType,
        encode: fn(u64, &M) -> RS<Vec<u8>>,
        decode: fn(&Frame) -> RS<M>,
    ) -> M {
        let encoded = encode(request_id, message).unwrap();
        let frame = Frame::decode(&encoded).unwrap();
        assert_eq!(frame.header().message_type(), expected);
        decode(&frame).unwrap()
    }

    #[test]
    fn repl_messages_roundtrip() {
        for case in [
            repl_subscribe_roundtrip,
            repl_subscribe_accept_roundtrip,
            repl_batch_roundtrip,
            repl_ack_roundtrip,
            repl_heal_fetch_roundtrip,
            repl_heal_response_roundtrip,
            repl_snapshot_offer_roundtrip,
            repl_snapshot_fetch_roundtrip,
            repl_snapshot_response_roundtrip,
        ] {
            case();
        }
    }

    fn repl_subscribe_roundtrip() {
        let subscribe = roundtrip_message(
            1,
            &ReplSubscribeReq::new_with_epoch(7, 2, 1024, 5),
            MessageType::ReplSubscribe,
            encode_repl_subscribe_request,
            decode_repl_subscribe_request,
        );
        assert_eq!(subscribe.src_node_id(), 7);
        assert_eq!(subscribe.worker_id(), 2);
        assert_eq!(subscribe.from_lsn(), 1024);
        assert_eq!(subscribe.src_epoch(), 5);
    }

    fn repl_subscribe_accept_roundtrip() {
        let accept = roundtrip_message(
            2,
            &ReplSubscribeAcceptFrame::new(9, 2, 6),
            MessageType::ReplSubscribeAccept,
            encode_repl_subscribe_accept_frame,
            decode_repl_subscribe_accept_frame,
        );
        assert_eq!(accept.node_id(), 9);
        assert_eq!(accept.worker_id(), 2);
        assert_eq!(accept.epoch(), 6);
    }

    fn repl_batch_roundtrip() {
        let batch = roundtrip_message(
            2,
            &ReplBatchFrame::new_with_epoch(3, 2048, vec![b"f1".to_vec(), b"f2".to_vec()], 6),
            MessageType::ReplBatch,
            encode_repl_batch_frame,
            decode_repl_batch_frame,
        );
        assert_eq!(batch.worker_id(), 3);
        assert_eq!(batch.lsn_lo(), 2048);
        assert_eq!(batch.epoch(), 6);
        assert_eq!(batch.frames(), &[b"f1".to_vec(), b"f2".to_vec()]);
        assert_eq!(batch.into_frames(), vec![b"f1".to_vec(), b"f2".to_vec()]);
    }

    fn repl_ack_roundtrip() {
        let ack = roundtrip_message(
            3,
            &ReplAckFrame::new(4, 4096),
            MessageType::ReplAck,
            encode_repl_ack_frame,
            decode_repl_ack_frame,
        );
        assert_eq!(ack.worker_id(), 4);
        assert_eq!(ack.stored_lsn(), 4096);
    }

    fn repl_heal_fetch_roundtrip() {
        let heal_fetch = roundtrip_message(
            4,
            &ReplHealFetchReq::new(5, 100, 200),
            MessageType::ReplHealFetch,
            encode_repl_heal_fetch_request,
            decode_repl_heal_fetch_request,
        );
        assert_eq!(heal_fetch.worker_id(), 5);
        assert_eq!(heal_fetch.lsn_lo(), 100);
        assert_eq!(heal_fetch.lsn_hi(), 200);
    }

    fn repl_heal_response_roundtrip() {
        let heal_resp = roundtrip_message(
            5,
            &ReplHealRespFrame::new(6, vec![b"g1".to_vec(), b"g2".to_vec()]),
            MessageType::ReplHealResp,
            encode_repl_heal_response_frame,
            decode_repl_heal_response_frame,
        );
        assert_eq!(heal_resp.worker_id(), 6);
        assert_eq!(heal_resp.frames(), &[b"g1".to_vec(), b"g2".to_vec()]);
        assert_eq!(
            heal_resp.into_frames(),
            vec![b"g1".to_vec(), b"g2".to_vec()]
        );
    }

    fn repl_snapshot_offer_roundtrip() {
        let manifest = ReplSnapshotManifest {
            version: 1,
            worker_index: 2,
            committed_lsn: 4095,
            seg_hash: [7u8; 32],
            raft_term: 3,
            raft_index: 42,
            taken_at_ms: 123456,
            files: vec![
                ReplSnapshotFileEntry {
                    path: "kv.mp".to_string(),
                    len: 100,
                    crc64: 111,
                },
                ReplSnapshotFileEntry {
                    path: "relation/0.1.0.dat".to_string(),
                    len: 4096,
                    crc64: 222,
                },
            ],
        };
        assert_eq!(manifest.resume_lsn(), 4096);
        let offer = roundtrip_message(
            6,
            &ReplSnapshotOfferFrame::new(2, manifest.clone()),
            MessageType::ReplSnapshotOffer,
            encode_repl_snapshot_offer_frame,
            decode_repl_snapshot_offer_frame,
        );
        assert_eq!(offer.worker_id(), 2);
        assert_eq!(offer.manifest(), &manifest);
        assert_eq!(offer.into_manifest(), manifest);
    }

    fn repl_snapshot_fetch_roundtrip() {
        let fetch = roundtrip_message(
            7,
            &ReplSnapshotFetchReq::new(2, 4095, "kv.mp".to_string(), 64, 4096),
            MessageType::ReplSnapshotFetch,
            encode_repl_snapshot_fetch_request,
            decode_repl_snapshot_fetch_request,
        );
        assert_eq!(fetch.worker_id(), 2);
        assert_eq!(fetch.snapshot_lsn(), 4095);
        assert_eq!(fetch.path(), "kv.mp");
        assert_eq!(fetch.offset(), 64);
        assert_eq!(fetch.len(), 4096);
    }

    fn repl_snapshot_response_roundtrip() {
        let resp = roundtrip_message(
            7,
            &ReplSnapshotRespFrame::new(2, "kv.mp".to_string(), 64, b"data".to_vec()),
            MessageType::ReplSnapshotResp,
            encode_repl_snapshot_resp_frame,
            decode_repl_snapshot_resp_frame,
        );
        assert_eq!(resp.worker_id(), 2);
        assert_eq!(resp.path(), "kv.mp");
        assert_eq!(resp.offset(), 64);
        assert_eq!(resp.data(), b"data");
        assert_eq!(resp.into_data(), b"data".to_vec());
    }

    #[test]
    fn raft_messages_roundtrip() {
        for case in [
            raft_pre_vote_request_roundtrip,
            raft_pre_vote_response_roundtrip,
            raft_vote_request_roundtrip,
            raft_vote_response_roundtrip,
            raft_append_request_roundtrip,
            raft_append_response_roundtrip,
            raft_apply_request_roundtrip,
            raft_apply_response_roundtrip,
        ] {
            case();
        }
    }

    fn raft_pre_vote_request_roundtrip() {
        let pre_vote = roundtrip_message(
            1,
            &RaftPreVoteReqFrame::new(7, 3, 2, 42),
            MessageType::RaftPreVoteReq,
            encode_raft_pre_vote_request,
            decode_raft_pre_vote_request,
        );
        assert_eq!(pre_vote.src_node_id(), 7);
        assert_eq!(pre_vote.request_term(), 3);
        assert_eq!(pre_vote.last_log_term(), 2);
        assert_eq!(pre_vote.last_log_index(), 42);
    }

    fn raft_pre_vote_response_roundtrip() {
        let pre_vote_resp = roundtrip_message(
            2,
            &RaftPreVoteRespFrame::new(8, 3, true),
            MessageType::RaftPreVoteResp,
            encode_raft_pre_vote_response,
            decode_raft_pre_vote_response,
        );
        assert_eq!(pre_vote_resp.src_node_id(), 8);
        assert_eq!(pre_vote_resp.request_term(), 3);
        assert!(pre_vote_resp.vote_granted());
    }

    fn raft_vote_request_roundtrip() {
        let vote = roundtrip_message(
            3,
            &RaftVoteReqFrame::new(9, 4, 2, 43),
            MessageType::RaftVoteReq,
            encode_raft_vote_request,
            decode_raft_vote_request,
        );
        assert_eq!(vote.src_node_id(), 9);
        assert_eq!(vote.term(), 4);
        assert_eq!(vote.last_log_term(), 2);
        assert_eq!(vote.last_log_index(), 43);
    }

    fn raft_vote_response_roundtrip() {
        let vote_resp = roundtrip_message(
            4,
            &RaftVoteRespFrame::new(10, 4, false),
            MessageType::RaftVoteResp,
            encode_raft_vote_response,
            decode_raft_vote_response,
        );
        assert_eq!(vote_resp.src_node_id(), 10);
        assert_eq!(vote_resp.term(), 4);
        assert!(!vote_resp.vote_granted());
    }

    fn raft_append_request_roundtrip() {
        let entries = vec![
            RaftLogEntryWire::new(4, 44, b"e1".to_vec()),
            RaftLogEntryWire::new(4, 45, b"e2".to_vec()),
        ];
        let append = roundtrip_message(
            5,
            &RaftAppendReqFrame::new(11, 4, 43, 2, entries.clone(), 40),
            MessageType::RaftAppendReq,
            encode_raft_append_request,
            decode_raft_append_request,
        );
        assert_eq!(append.src_node_id(), 11);
        assert_eq!(append.term(), 4);
        assert_eq!(append.prev_log_index(), 43);
        assert_eq!(append.prev_log_term(), 2);
        assert_eq!(append.commit_index(), 40);
        assert_eq!(append.entries(), entries.as_slice());
        assert_eq!(append.into_entries(), entries);
    }

    fn raft_append_response_roundtrip() {
        let append_resp = roundtrip_message(
            6,
            &RaftAppendRespFrame::new(12, 4, true, 40, 45, 0),
            MessageType::RaftAppendResp,
            encode_raft_append_response,
            decode_raft_append_response,
        );
        assert_eq!(append_resp.src_node_id(), 12);
        assert_eq!(append_resp.term(), 4);
        assert!(append_resp.append_success());
        assert_eq!(append_resp.commit_index(), 40);
        assert_eq!(append_resp.match_index(), 45);
        assert_eq!(append_resp.next_index(), 0);
    }

    fn raft_apply_request_roundtrip() {
        let apply = roundtrip_message(
            7,
            &RaftApplyReqFrame::new(13, 5, "snap-1".to_string(), 1, 50, 50, 4),
            MessageType::RaftApplyReq,
            encode_raft_apply_request,
            decode_raft_apply_request,
        );
        assert_eq!(apply.src_node_id(), 13);
        assert_eq!(apply.term(), 5);
        assert_eq!(apply.id(), "snap-1");
        assert_eq!(apply.begin_index(), 1);
        assert_eq!(apply.end_index(), 50);
        assert_eq!(apply.snapshot_index(), 50);
        assert_eq!(apply.snapshot_term(), 4);
    }

    fn raft_apply_response_roundtrip() {
        let apply_resp = roundtrip_message(
            8,
            &RaftApplyRespFrame::new(14, 5, 50, "snap-1".to_string()),
            MessageType::RaftApplyResp,
            encode_raft_apply_response,
            decode_raft_apply_response,
        );
        assert_eq!(apply_resp.src_node_id(), 14);
        assert_eq!(apply_resp.term(), 5);
        assert_eq!(apply_resp.match_index(), 50);
        assert_eq!(apply_resp.id(), "snap-1");
    }

    #[test]
    fn raft_log_entry_wire_accessors() {
        let entry = RaftLogEntryWire::new(3, 17, b"payload".to_vec());
        assert_eq!(entry.term(), 3);
        assert_eq!(entry.index(), 17);
        assert_eq!(entry.payload(), b"payload");
        assert_eq!(entry.into_payload(), b"payload".to_vec());
    }

    #[test]
    fn error_response_leader_hint_roundtrip() {
        let hint = LeaderHint::new("primary.internal", 5440);
        assert_eq!(hint.host(), "primary.internal");
        assert_eq!(hint.port(), 5440);

        let error = ErrorResponse::new("not the leader").with_leader_hint(hint.clone());
        let payload = rmp_serde::to_vec(&error).unwrap();
        let frame = Frame::new(MessageType::Error, 7, payload);
        let decoded = decode_error_response(&frame).unwrap();
        assert_eq!(decoded.leader_hint(), Some(&hint));

        let without_hint = ErrorResponse::new("boom");
        assert_eq!(without_hint.leader_hint(), None);
        let payload = rmp_serde::to_vec(&without_hint).unwrap();
        let frame = Frame::new(MessageType::Error, 8, payload);
        let decoded = decode_error_response(&frame).unwrap();
        assert_eq!(decoded.leader_hint(), None);
    }

    #[test]
    fn error_response_decodes_legacy_payload_without_leader_hint() {
        // Payloads encoded before the `leader_hint` field existed hold only
        // (code, name, message, source, location); the serde default must fill
        // `leader_hint` with `None`.
        let legacy_payload = rmp_serde::to_vec(&(
            ErrorCode::Parse.to_u32(),
            "Parse".to_string(),
            "bad request".to_string(),
            String::new(),
            String::new(),
        ))
        .unwrap();
        let frame = Frame::new(MessageType::Error, 9, legacy_payload);
        let decoded = decode_error_response(&frame).unwrap();
        assert_eq!(decoded.code(), ErrorCode::Parse.to_u32());
        assert_eq!(decoded.message(), "bad request");
        assert_eq!(decoded.leader_hint(), None);
    }
}
