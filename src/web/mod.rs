use std::{net::SocketAddr, sync::Arc};

use axum::Router;
use tokio::{
    net::{TcpListener, UnixListener},
    signal::{
        ctrl_c,
        unix::{self, SignalKind},
    },
    task::JoinHandle,
};

use crate::config::{Config, WebData};

mod image;
mod paste;
mod static_files;

pub fn start_web(
    mut config: Config,
    webdata: Arc<WebData>,
) -> JoinHandle<Result<(), std::io::Error>> {
    let bind_addr = config.get_bind_addr();
    let ratelim = config.ratelim.take();
    if !config.link_prefix.is_empty() {
        println!("Listening on {}", config.link_prefix);
    }

    let web = Router::<Arc<WebData>>::new()
        .merge(static_files::routes())
        .merge(image::routes(webdata.image.get_base(), ratelim))
        .with_state(webdata);
    tokio::spawn(async move {
        let shutdown_h = async move {
            let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
        };
        if let Some(unix) = bind_addr.strip_prefix("unix:") {
            let uds = UnixListener::bind(unix)?;
            axum::serve(uds, web)
                .with_graceful_shutdown(shutdown_h)
                .await
        } else {
            let inet = TcpListener::bind(bind_addr).await?;
            axum::serve(
                inet,
                web.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_h)
            .await
        }
    })
}
