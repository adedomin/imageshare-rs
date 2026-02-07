#[cfg(unix)]
pub async fn shutdown() {
    use tokio::signal::{
        ctrl_c,
        unix::{self, SignalKind},
    };

    let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = ctrl_c() => {}
        _ = sigterm.recv() => {}
    }
    eprintln!("WARN: Shutting down.");
}

#[cfg(windows)]
pub mod windows {
    use std::sync::OnceLock;

    use tokio_util::sync::CancellationToken;

    pub static SERVICE_STOP: OnceLock<CancellationToken> = OnceLock::new();
}

#[cfg(windows)]
pub async fn shutdown() {
    use crate::shutdown::windows::SERVICE_STOP;
    use tokio::signal::windows;

    if let Some(stop) = SERVICE_STOP.get() {
        // running as a Windows service
        stop.cancelled().await
    } else {
        // conhost
        let mut ctrl_break = windows::ctrl_break().unwrap();
        let mut ctrl_c = windows::ctrl_c().unwrap();
        let mut ctrl_close = windows::ctrl_close().unwrap();
        let mut ctrl_logoff = windows::ctrl_logoff().unwrap();
        let mut ctrl_shutdown = windows::ctrl_shutdown().unwrap();
        tokio::select! {
            _ = ctrl_break.recv() => {
                eprintln!("Ctrl-BREAK: shutting down.");
            }
            _ = ctrl_c.recv() => {
                eprintln!("Ctrl-C: shutting down.");
            }
            _ = ctrl_close.recv() => {
                eprintln!("Ctrl-Close: shutting down.");
            }
            _ = ctrl_logoff.recv() => {
                eprintln!("Ctrl-Logoff: shutting down.");
            }
            _ = ctrl_shutdown.recv() => {
                eprintln!("Ctrl-Shutdown: shutting down.");
            }
        }
    }
}
