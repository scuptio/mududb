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
use crate::task::spawn_local_task;
use mudu::common::result::RS;
#[cfg(feature = "debug_trace")]
use mudu::error::ec::EC;
use mudu::error::err::MError;
#[cfg(feature = "debug_trace")]
use mudu::m_error;
#[cfg(feature = "debug_trace")]
use tokio::net::TcpListener;
#[cfg(feature = "debug_trace")]
use tokio::task::LocalSet;
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
pub async fn async_debug_serve(addr: SocketAddr) -> Result<(), MError> {
    let port = addr.port();
    let r = SERVER.insert_sync(port);
    if r.is_err() {
        return Err(m_error!(EC::ExistingSuchElement, ""));
    }

    // Bind to the port and listen for incoming TCP connections
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| m_error!(EC::IOErr, "bind to address error", e))?;
    loop {
        // When an incoming TCP connection is received grab a TCP stream for
        // client<->server communication.
        //
        // Note, this is a .await point, this loop will loop forever but is not a busy loop. The
        // .await point allows the Tokio runtime to pull the task off of the thread until the task
        // has work to do. In this case, a connection arrives on the port we are listening on and
        // the task is woken up, at which point the task is then put back on a thread, and is
        // driven forward by the runtime, eventually yielding a TCP stream.
        let (tcp, _) = listener
            .accept()
            .await
            .map_err(|e| m_error!(EC::IOErr, "accept error", e))?;
        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(tcp);

        // Spin up a new task in Tokio so we can continue to listen for new TCP connection on the
        // current task without waiting for the processing of the HTTP1 connection we just received
        // to finish
        tokio::task::spawn(async move {
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
}

#[cfg(not(feature = "debug_trace"))]
pub async fn async_debug_serve(_addr: SocketAddr) -> Result<(), MError> {
    Ok(())
}

#[cfg(feature = "debug_trace")]
pub fn debug_serve(canceler: NotifyWait, port: u16) {
    let async_debug_serve = async_debug_serve(([0, 0, 0, 0], port).into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ls = LocalSet::new();
    runtime.block_on(async {
        ls.spawn_local(async move {
            let _ = spawn_local_task(canceler, "debug_server", async_debug_serve);
        });
        ls.await;
    });
}

#[cfg(not(feature = "debug_trace"))]
pub fn debug_serve(_canceler: NotifyWait, _port: u16) {}
