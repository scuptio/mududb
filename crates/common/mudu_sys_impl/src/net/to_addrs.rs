/// Anything that can be resolved into socket addresses (host names, address
/// strings, or ready-made socket addresses); mirrors
/// [`tokio::net::ToSocketAddrs`].
pub trait ToAddrs: tokio::net::ToSocketAddrs {}

impl<T: tokio::net::ToSocketAddrs> ToAddrs for T {}
