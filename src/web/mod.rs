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
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::Router;
use tokio::{net::TcpListener, task::JoinHandle};
use tower::ServiceBuilder;

use crate::{
    config::Config,
    middleware::{csrf::HeaderCsrf, ratelim::BucketRatelim},
    models::webdata::WebData,
    shutdown::shutdown,
    web::uds::UdsErr,
};

mod image;
mod paste;
mod static_files;
mod uds;

#[derive(Debug, thiserror::Error)]
pub enum WebErr {
    #[error("UNIX Socket: {0}")]
    Uds(#[from] UdsErr),
    #[error("Could not listen on: {0} -- Reason: {0}")]
    InetFail(String, std::io::Error),
    #[error("Unknown I/O Error: {0}")]
    GenericIO(#[from] std::io::Error),
}

pub fn start_web(mut config: Config, webdata: Arc<WebData>) -> JoinHandle<Result<(), WebErr>> {
    let bind_addr = config.get_bind_addr();
    let ratelim = config.ratelim.take().map(BucketRatelim::from);
    if !config.link_prefix.is_empty() {
        println!("Listening on {}", config.link_prefix);
    }

    let web = Router::<Arc<WebData>>::new()
        .merge(image::upload_route(webdata.image.get_max_siz()))
        .merge(paste::upload_route(webdata.paste.get_max_siz()))
        .layer(
            ServiceBuilder::new()
                .layer(HeaderCsrf)
                .option_layer(ratelim),
        )
        .merge(image::serve_route(webdata.image.get_base()))
        .merge(paste::serve_route(webdata.paste.get_base()))
        .merge(static_files::routes())
        .with_state(webdata);
    let shutdown_h = shutdown();
    tokio::spawn(async move {
        #[allow(unused_variables)]
        if let Some(unix) = bind_addr.strip_prefix("unix:").map(PathBuf::from) {
            #[cfg(unix)]
            {
                use crate::web::uds::unix::listen_uds;

                let uds = listen_uds(unix).await?;
                axum::serve(uds, web)
                    .with_graceful_shutdown(shutdown_h)
                    .await
                    .map_err(|e| e.into())
            }
            #[cfg(windows)]
            {
                Err(WebErr::Uds(UdsErr::Windows))
            }
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
