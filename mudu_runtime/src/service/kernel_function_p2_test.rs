#[cfg(test)]
mod tests {
    use crate::service::kernel_function_p2::{
        host_batch, host_close, host_command, host_delete, host_fetch, host_get, host_open,
        host_put, host_query, host_range,
    };
    use mudu::common::serde_utils::deserialize_from;
    use mudu_binding::codec::handle_sys_session;
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_error::UniError;

    const MERR_MAGIC: &[u8] = b"MERR";

    fn decode_merr_payload(bytes: &[u8]) -> UniError {
        assert!(bytes.starts_with(MERR_MAGIC));
        deserialize_from::<UniError>(&bytes[MERR_MAGIC.len()..])
            .map(|(e, _)| e)
            .expect("valid MERR payload")
    }

    fn assert_worker_local_error(bytes: &[u8]) {
        let err = decode_merr_payload(bytes);
        assert!(
            err.err_msg
                .contains("worker local interface is not configured"),
            "unexpected error message: {}",
            err.err_msg
        );
    }

    #[test]
    fn host_open_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_open_param();
        let output = host_open(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn host_close_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_close_param(1);
        let output = host_close(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn host_get_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let output = host_get(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn host_put_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_put_param(1, b"alpha", b"beta");
        let output = host_put(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn host_delete_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let output = host_delete(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[test]
    fn host_range_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_range_param(1, b"a", b"z");
        let output = host_range(input, None);
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    fn assert_no_session_error(bytes: &[u8]) {
        let err = query_invoke::deserialize_query_result(bytes)
            .err()
            .or_else(|| command_invoke::deserialize_command_result(bytes).err())
            .expect("result should be an error");
        assert!(
            err.to_string().contains("no such session id"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn host_query_without_session_returns_no_session_error() {
        let input = query_invoke::serialize_query_dyn_param(12345, &"SELECT 1", &())
            .expect("serialize query param");
        let output = host_query(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn host_command_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let output = host_command(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn host_batch_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let output = host_batch(input);
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[test]
    fn host_fetch_empty_returns_empty() {
        assert!(host_fetch(vec![]).is_empty());
    }

    #[test]
    fn host_query_malformed_input_does_not_panic_and_returns_error_payload() {
        let output = host_query(vec![0xff, 0x00, 0xab, 0xcd]);
        assert!(
            !output.is_empty(),
            "malformed input should yield an error payload"
        );
        // The query serializer produces a UniResult, so it won't have the MERR prefix.
        // Just ensure the payload is non-empty and decoding it fails gracefully.
        assert!(query_invoke::deserialize_query_result(&output).is_err());
    }
}
