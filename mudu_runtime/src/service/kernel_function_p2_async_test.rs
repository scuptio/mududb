#[cfg(test)]
mod tests {
    use crate::service::kernel_function_p2_async::{
        async_host_batch, async_host_close, async_host_command, async_host_delete,
        async_host_fetch, async_host_get, async_host_open, async_host_put, async_host_query,
        async_host_range,
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

    #[tokio::test]
    async fn async_host_open_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_open_param();
        let output = async_host_open(input, None).await;
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[tokio::test]
    async fn async_host_close_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_close_param(1);
        let output = async_host_close(input, None).await;
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[tokio::test]
    async fn async_host_get_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_get_param(1, b"alpha");
        let output = async_host_get(input, None).await;
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[tokio::test]
    async fn async_host_put_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_put_param(1, b"alpha", b"beta");
        let output = async_host_put(input, None).await;
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[tokio::test]
    async fn async_host_delete_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_delete_param(1, b"alpha");
        let output = async_host_delete(input, None).await;
        assert!(output.starts_with(MERR_MAGIC));
        assert_worker_local_error(&output);
    }

    #[tokio::test]
    async fn async_host_range_without_worker_local_returns_decodable_error() {
        let input = handle_sys_session::serialize_session_range_param(1, b"a", b"z");
        let output = async_host_range(input, None).await;
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

    #[tokio::test]
    async fn async_host_query_without_session_returns_no_session_error() {
        let input = query_invoke::serialize_query_dyn_param(12345, &"SELECT 1", &())
            .expect("serialize query param");
        let output = async_host_query(input).await;
        assert!(!output.is_empty());
        assert_no_session_error(&output);
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
