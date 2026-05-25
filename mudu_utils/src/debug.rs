// Example hyper http server
// https://github.com/hyperium/hyper/blob/master/examples/echo.rs

use std::net::SocketAddr;

#[cfg(feature = "debug_trace")]
use bytes::Bytes;
#[cfg(feature = "debug_trace")]
use http::{Method, StatusCode};
#[cfg(feature = "debug_trace")]
use http_body_util::Full;
#[cfg(feature = "debug_trace")]
use hyper::body::Incoming;
#[cfg(feature = "debug_trace")]
use hyper::server::conn::http1;
#[cfg(feature = "debug_trace")]
use hyper::service::service_fn;
#[cfg(feature = "debug_trace")]
use hyper::{Request, Response};
#[cfg(feature = "debug_trace")]
use hyper_util::rt::TokioIo;
#[cfg(feature = "debug_trace")]
use lazy_static::lazy_static;
#[cfg(feature = "debug_trace")]
use scc::{HashIndex, HashSet};

#[cfg(feature = "debug_trace")]
use crate::dump_task_trace;
use crate::notifier::NotifyWait;
#[cfg(feature = "debug_trace")]
use crate::task_async::CurrentThreadTaskRuntime;
use mudu::common::result::RS;
#[cfg(feature = "debug_trace")]
use mudu::error::ec::EC;
use mudu::error::err::MError;
#[cfg(feature = "debug_trace")]
use mudu::m_error;
#[cfg(feature = "debug_trace")]
use mudu_sys::tokio::net::TcpListener;
#[cfg(feature = "debug_trace")]
use mudu_sys::tokio::task::JoinSet;
#[cfg(feature = "debug_trace")]
use tracing::error;

#[cfg(feature = "debug_trace")]
type HandleURL = fn(String) -> RS<String>;

#[cfg(feature = "debug_trace")]
lazy_static! {
    static ref HANDLE_URL: HashIndex<String, HandleURL> = HashIndex::new();
    static ref SERVER: HashSet<u16> = HashSet::new();
}

#[cfg(feature = "debug_trace")]
pub fn register_debug_url(url: String, h: HandleURL) {
    let _ = HANDLE_URL.insert_sync(url, h);
}

#[cfg(not(feature = "debug_trace"))]
pub fn register_debug_url(_url: String, _h: fn(String) -> RS<String>) {}

#[cfg(feature = "debug_trace")]
async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let mut response = Response::new(Full::default());
    match req.method() {
        &Method::GET => {
            let path = req.uri().path();
            match path {
                "/task" | "/" | "" => {
                    let dump = dump_task_trace!();
                    *response.body_mut() = Full::from(dump);
                }
                _ => {
                    let opt = HANDLE_URL.get_sync(&path.to_string());
                    match opt {
                        Some(e) => {
                            let h = *e.get();
                            let s = h(path.to_string()).unwrap_or_else(|e| e.to_string());
                            *response.body_mut() = Full::from(s);
                        }
                        None => {
                            *response.status_mut() = StatusCode::NOT_FOUND;
                        }
                    }
                }
            }
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
        }
    }
    Ok(response)
}

#[cfg(feature = "debug_trace")]
pub async fn async_debug_serve_until(addr: SocketAddr, stop: NotifyWait) -> Result<(), MError> {
    crate::scoped_task_trace!();
    let port = addr.port();
    let r = SERVER.insert_sync(port);
    if r.is_err() {
        return Err(m_error!(EC::ExistingSuchElement, ""));
    }

    // Bind to the port and listen for incoming TCP connections
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            let _ = SERVER.remove_sync(&port);
            return Err(m_error!(EC::IOErr, "bind to address error", e));
        }
    };
    let mut tasks = JoinSet::new();
    loop {
        let accepted = mudu_sys::tokio::select! {
            _ = stop.notified() => {
                break;
            }
            accepted = listener.accept() => accepted
        };
        let (tcp, _) = match accepted {
            Ok(accepted) => accepted,
            Err(e) => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                let _ = SERVER.remove_sync(&port);
                return Err(m_error!(EC::IOErr, "accept error", e));
            }
        };
        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(tcp);

        tasks.spawn(async move {
            // Handle the connection from the client using HTTP1 and pass any
            // HTTP requests received on that connection to the `hello` function
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(handle_request))
                .await
            {
                error!("Error serving connection: {:?}", err);
            }
        });
    }

    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    let _ = SERVER.remove_sync(&port);
    Ok(())
}

#[cfg(feature = "debug_trace")]
pub async fn async_debug_serve(addr: SocketAddr) -> Result<(), MError> {
    async_debug_serve_until(addr, NotifyWait::new()).await
}

#[cfg(not(feature = "debug_trace"))]
pub async fn async_debug_serve_until(_addr: SocketAddr, _stop: NotifyWait) -> Result<(), MError> {
    Ok(())
}

#[cfg(not(feature = "debug_trace"))]
pub async fn async_debug_serve(_addr: SocketAddr) -> Result<(), MError> {
    Ok(())
}

#[cfg(feature = "debug_trace")]
pub fn debug_serve(canceler: NotifyWait, port: u16) {
    let async_debug_serve = async_debug_serve_until(([0, 0, 0, 0], port).into(), canceler.clone());
    let runtime = CurrentThreadTaskRuntime::new().unwrap();
    let join = runtime
        .local()
        .spawn(canceler, "debug_server", async_debug_serve)
        .unwrap();
    runtime.block_on(async {
        let _ = join.await;
    });
}

#[cfg(not(feature = "debug_trace"))]
pub fn debug_serve(_canceler: NotifyWait, _port: u16) {}
