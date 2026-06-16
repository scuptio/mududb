pub(crate) use crate::contract::to_addrs::ToAddrs;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::net::SocketAddr;

pub async fn lookup_host<A: ToAddrs>(addr: A) -> RS<Vec<SocketAddr>> {
    tokio::net::lookup_host(addr)
        .await
        .map_err(|e| m_error!(EC::NetErr, "resolve address error", e))
        .map(|iter| iter.collect())
}
