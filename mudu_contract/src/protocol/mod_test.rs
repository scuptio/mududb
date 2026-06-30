//! Tests for the protocol module.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(test)]
mod tests {
    use super::super::*;
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
        use mudu_type::dat_type::DatType;
        use mudu_type::dat_type_id::DatTypeID;
        use mudu_type::dat_value::DatValue;

        let response = ServerResponse::new(
            TupleFieldDesc::new(vec![DatumDesc::new(
                "value".to_string(),
                DatType::default_for(DatTypeID::String),
            )]),
            vec![TupleValue::from(vec![DatValue::from_string(
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
        ];
        for (value, expected) in cases {
            assert_eq!(MessageType::try_from(value).unwrap(), expected);
            assert_eq!(u32::from(expected), value);
        }
        assert!(MessageType::try_from(0).is_err());
        assert!(MessageType::try_from(14).is_err());
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
    fn server_response_accessors_and_perf_digest() {
        use crate::tuple::datum_desc::DatumDesc;
        use crate::tuple::tuple_field_desc::TupleFieldDesc;
        use crate::tuple::tuple_value::TupleValue;
        use mudu_type::dat_type::DatType;
        use mudu_type::dat_type_id::DatTypeID;
        use mudu_type::dat_value::DatValue;

        let row_desc = TupleFieldDesc::new(vec![DatumDesc::new(
            "c".to_string(),
            DatType::default_for(DatTypeID::String),
        )]);
        let rows = vec![TupleValue::from(vec![DatValue::from_string(
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
}
