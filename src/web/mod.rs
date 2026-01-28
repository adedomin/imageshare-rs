// Copyright (c) 2026, Anthony DeDominic <adedomin@gmail.com>
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
use std::{io::ErrorKind, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::Router;
use tokio::{
    fs::{create_dir_all, remove_file},
    net::{TcpListener, UnixListener},
    signal::{
        ctrl_c,
        unix::{self, SignalKind},
    },
    task::JoinHandle,
};
use tower::ServiceBuilder;

use crate::{
    config::Config,
    middleware::{csrf::HeaderCsrf, ratelim::BucketRatelim},
    models::webdata::WebData,
};

mod image;
mod paste;
mod static_files;

#[derive(Debug, thiserror::Error)]
pub enum WebErr {
    #[error("Could not bind socket: {0:?} -- Reason: {1}")]
    UknkUdsBind(PathBuf, std::io::Error),
    #[error("Could not create destion for socket: {0:?} -- Reason: {1}")]
    CreateParents(PathBuf, std::io::Error),
    #[error("Could not create destion for socket: {0:?} -- Reason: no parents?!")]
    NoParents(PathBuf),
    #[error("Could not remove stale socket: {0:?} -- Reason: {1}")]
    RemoveStale(PathBuf, std::io::Error),
    #[error("Could not listen on: {0} -- Reason: {0}")]
    InetFail(String, std::io::Error),
    #[error("Unknown I/O Error: {0}")]
    GenericIO(#[from] std::io::Error),
    #[error("Fail looping making {0:?}")]
    Loop(PathBuf),
}

pub fn start_web(mut config: Config, webdata: Arc<WebData>) -> JoinHandle<Result<(), WebErr>> {
    let bind_addr = config.get_bind_addr();
    let ratelim = config.ratelim.take();
    if !config.link_prefix.is_empty() {
        println!("Listening on {}", config.link_prefix);
    }

    let web = Router::<Arc<WebData>>::new()
        .merge(image::routes(webdata.clone()))
        .merge(paste::routes(webdata.clone()))
        .layer(
            ServiceBuilder::new()
                .layer(HeaderCsrf)
                .option_layer(ratelim.map(BucketRatelim::from)),
        )
        .merge(static_files::routes())
        .with_state(webdata);
    tokio::spawn(async move {
        let shutdown_h = async move {
            let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            eprintln!("WARN: Shutting down.");
        };
        if let Some(unix) = bind_addr.strip_prefix("unix:").map(PathBuf::from) {
            let mut loop_ctr = 0;
            let uds = loop {
                if loop_ctr == 2 {
                    return Err(WebErr::Loop(unix.to_path_buf()));
                }
                loop_ctr += 1;
                match UnixListener::bind(&unix) {
                    Ok(uds) => break uds,
                    Err(e) if e.kind() == ErrorKind::NotFound => {
                        if let Some(parent) = unix.parent() {
                            create_dir_all(parent)
                                .await
                                .map_err(|e| WebErr::CreateParents(unix.to_path_buf(), e))?;
                        } else {
                            return Err(WebErr::NoParents(unix.to_path_buf()));
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::AddrInUse => {
                        remove_file(&unix)
                            .await
                            .map_err(|e| WebErr::RemoveStale(unix.to_path_buf(), e))?;
                    }
                    Err(e) => return Err(WebErr::UknkUdsBind(unix.to_path_buf(), e)),
                }
            };
            axum::serve(uds, web)
                .with_graceful_shutdown(shutdown_h)
                .await
                .map_err(|e| e.into())
        } else {
            let inet = TcpListener::bind(&bind_addr)
                .await
                .map_err(|e| WebErr::InetFail(bind_addr, e))?;
            axum::serve(
                inet,
                web.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_h)
            .await
            .map_err(|e| e.into())
        }
    })
}
