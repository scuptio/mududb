#[cfg(test)]
mod tests {
    use crate::service::kernel_function_p2_async::{
        async_host_batch, async_host_close, async_host_command, async_host_delete,
        async_host_fetch, async_host_get, async_host_open, async_host_put, async_host_query,
        async_host_range,
    };
    use mudu_binding::codec::syscall_payload::{
        decode_close_result, decode_delete_result, decode_get_result, decode_open_result,
        decode_put_result, decode_range_result, encode_close_request, encode_delete_request,
        encode_get_request, encode_open_request, encode_put_request, encode_range_request,
    };
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_oid::UniOid;

    fn assert_worker_local_error(message: &str) {
        assert!(
            message.contains("worker local interface is not configured"),
            "unexpected error message: {message}"
        );
    }

    #[tokio::test]
    async fn async_host_open_without_worker_local_returns_decodable_error() {
        let input = encode_open_request(UniOid::from_oid(0));
        let output = async_host_open(input, None).await;
        let err = decode_open_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[tokio::test]
    async fn async_host_close_without_worker_local_returns_decodable_error() {
        let input = encode_close_request(UniOid::from_oid(1));
        let output = async_host_close(input, None).await;
        let err = decode_close_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[tokio::test]
    async fn async_host_get_without_worker_local_returns_decodable_error() {
        let input = encode_get_request(UniOid::from_oid(1), b"alpha");
        let output = async_host_get(input, None).await;
        let err = decode_get_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[tokio::test]
    async fn async_host_put_without_worker_local_returns_decodable_error() {
        let input = encode_put_request(UniOid::from_oid(1), b"alpha", b"beta");
        let output = async_host_put(input, None).await;
        let err = decode_put_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[tokio::test]
    async fn async_host_delete_without_worker_local_returns_decodable_error() {
        let input = encode_delete_request(UniOid::from_oid(1), b"alpha");
        let output = async_host_delete(input, None).await;
        let err = decode_delete_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    #[tokio::test]
    async fn async_host_range_without_worker_local_returns_decodable_error() {
        let input = encode_range_request(UniOid::from_oid(1), b"a", b"z");
        let output = async_host_range(input, None).await;
        let err = decode_range_result(&output).unwrap_err();
        assert_worker_local_error(err.message());
    }

    fn assert_no_session_error(bytes: &[u8]) {
        // The output is a command-kind MSSP frame (the batch handler shares
        // the command result serializer); decode it with the matching kind.
        let err = command_invoke::deserialize_command_result(bytes)
            .expect_err("result should be an error");
        assert!(
            err.to_string().contains("no such session id"),
            "unexpected error: {}",
            err
        );
    }

    fn assert_no_session_query_error(bytes: &[u8]) {
        let err = query_invoke::deserialize_query_result(bytes)
            .err()
            .expect("result should be an error");
        assert!(
            err.to_string().contains("no such session id"),
            "unexpected error: {}",
            err
        );
    }

    #[tokio::test]
    async fn async_host_query_without_session_returns_no_session_error() {
        let input = query_invoke::serialize_query_dyn_param(12345, &"SELECT 1", &())
            .expect("serialize query param");
        let output = async_host_query(input).await;
        assert!(!output.is_empty());
        assert_no_session_query_error(&output);
    }

    #[tokio::test]
    async fn async_host_command_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let output = async_host_command(input).await;
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[tokio::test]
    async fn async_host_batch_without_session_returns_no_session_error() {
        let input =
            command_invoke::serialize_command_param(12345, &"INSERT INTO t VALUES (1)", &())
                .expect("serialize command param");
        let output = async_host_batch(input).await;
        assert!(!output.is_empty());
        assert_no_session_error(&output);
    }

    #[tokio::test]
    async fn async_host_fetch_empty_returns_empty() {
        assert!(async_host_fetch(vec![]).await.is_empty());
    }

    #[tokio::test]
    async fn async_host_query_malformed_input_does_not_panic_and_returns_error_payload() {
        let output = async_host_query(vec![0xff, 0x00, 0xab, 0xcd]).await;
        assert!(
            !output.is_empty(),
            "malformed input should yield an error payload"
        );
        assert!(query_invoke::deserialize_query_result(&output).is_err());
    }
}
