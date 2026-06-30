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
