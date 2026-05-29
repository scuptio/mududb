use mudu::common::result::RS;
use mudu_cli::client::client::SyncClient;
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

pub fn reserve_port() -> RS<Option<u16>> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Ok(Some(
            listener
                .local_addr()
                .map_err(|e| {
                    mudu::m_error!(mudu::error::ec::EC::NetErr, "read local addr error", e)
                })?
                .port(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(None),
        Err(e) => Err(mudu::m_error!(
            mudu::error::ec::EC::NetErr,
            "reserve local tcp port error",
            e
        )),
    }
}

pub fn reserve_port_block(count: usize) -> RS<Option<u16>> {
    if count == 0 {
        return Ok(None);
    }
    for _ in 0..128 {
        let Some(base_port) = reserve_port()? else {
            return Ok(None);
        };
        let mut listeners = Vec::with_capacity(count);
        let mut ok = true;
        for offset in 0..count {
            let Some(port) = base_port.checked_add(offset as u16) else {
                ok = false;
                break;
            };
            match TcpListener::bind(("127.0.0.1", port)) {
                Ok(listener) => listeners.push(listener),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Ok(Some(base_port));
        }
    }
    Ok(None)
}

pub fn wait_until_port_ready(port: u16, service_name: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
    while mudu_sys::time::instant_now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        mudu_sys::task_sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::m_error!(
        mudu::error::ec::EC::NetErr,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

pub fn connect_sync_client_with_retry(port: u16) -> RS<SyncClient> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
    let mut last_err = None;
    while mudu_sys::time::instant_now() < deadline {
        match SyncClient::connect(("127.0.0.1", port)) {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_err = Some(err);
                mudu_sys::task_sync::sleep_blocking(Duration::from_millis(50));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        mudu::m_error!(
            mudu::error::ec::EC::NetErr,
            format!("timed out connecting SyncClient to TCP port {}", port)
        )
    }))
}
