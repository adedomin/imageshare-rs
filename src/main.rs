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
use std::process::exit;

use crate::{
    config::{ConfigError, get_config},
    web::WebErr,
};
mod config;
mod middleware;
mod models;
mod shutdown;
mod web;

#[derive(thiserror::Error, Debug)]
enum MainErr {
    #[error("{0}\n\nusage: imageshare-rs [ config.json ]")]
    Cfg(#[from] ConfigError),
    #[error("{0}")]
    Web(#[from] WebErr),
}

#[cfg(unix)]
fn main() {
    if let Err(e) = real_main() {
        eprintln!("{e}");
        exit(1);
    }
}

fn real_main() -> Result<(), MainErr> {
    let (config, webdata) = get_config()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap();
    Ok(rt.block_on(async {
        let web = web::start_web(config, webdata);
        web.await.unwrap()
    })?)
}

#[cfg(windows)]
fn main() {
    if svc_main().is_err()
        && let Err(e) = real_main()
    {
        eprintln!("{e}");
        exit(1);
    }
}

#[cfg(windows)]
fn svc_main() -> Result<(), ()> {
    use crate::shutdown::windows::SERVICE_STOP;
    use tokio_util::sync::CancellationToken;
    use windows_services::{Command, Service, State};

    let stop_token = CancellationToken::new();
    let mut thread = None;
    Service::new()
        .can_stop()
        .run(move |service, msg| match msg {
            Command::Start if thread.is_none() => {
                _ = SERVICE_STOP.set(stop_token.clone());
                thread = Some(unsafe {
                    std::thread::Builder::new()
                        .spawn_unchecked(move || {
                            real_main().inspect_err(|_| service.set_state(State::Stopped))
                        })
                        .unwrap()
                })
            }
            Command::Stop => {
                if let Some(jh) = thread.take() {
                    stop_token.cancel();
                    _ = jh.join();
                }
            }
            _ => (), // unsupported
        })
        .map_err(|_| ()) // err is static string.
}
