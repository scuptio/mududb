pub(crate) use crate::net::to_addrs::ToAddrs;
use mudu::common::result::RS;
use mudu::error::others::network_error_with_message;
use std::net::SocketAddr;

pub async fn lookup_host<A: ToAddrs>(addr: A) -> RS<Vec<SocketAddr>> {
    tokio::net::lookup_host(addr)
        .await
        .map_err(|e| network_error_with_message(e, "resolve address error"))
        .map(|iter| iter.collect())
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn resolve_numeric_address_and_reject_missing_port() {
        assert_eq!(
            super::lookup_host("127.0.0.1:1234").await.unwrap(),
            vec!["127.0.0.1:1234".parse::<std::net::SocketAddr>().unwrap()]
        );
        assert!(super::lookup_host("127.0.0.1").await.is_err());
    }
}
