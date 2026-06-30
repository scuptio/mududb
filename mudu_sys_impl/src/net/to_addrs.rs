pub trait ToAddrs: tokio::net::ToSocketAddrs {}

impl<T: tokio::net::ToSocketAddrs> ToAddrs for T {}
