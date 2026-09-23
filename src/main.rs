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

use tokio_util::sync::CancellationToken;

use crate::{
    config::{ConfigError, get_config},
    tasks::web::WebErr,
};

mod config;
mod middleware;
mod models;
mod tasks;
mod web;

#[derive(thiserror::Error, Debug)]
enum MainErr {
    #[error("{0}\n\nusage: imageshare-rs [ config.json ]")]
    Cfg(#[from] ConfigError),
    #[error("{0}")]
    Web(#[from] WebErr),
}

#[cfg(unix)]
fn set_rlimit() -> Result<(), libc::c_int> {
    use libc::{getrlimit, rlimit, setrlimit};

    let mut rlim = rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let rlim_ret = unsafe { getrlimit(libc::RLIMIT_NOFILE, &raw mut rlim) };
    if rlim_ret != 0 {
        return Err(rlim_ret);
    };

    if rlim.rlim_cur >= rlim.rlim_max {
        return Ok(());
    }
    rlim.rlim_cur = rlim.rlim_max;

    let rlim_ret = unsafe { setrlimit(libc::RLIMIT_NOFILE, &raw const rlim) };
    if rlim_ret != 0 {
        return Err(rlim_ret);
    };

    Ok(())
}

#[cfg(unix)]
fn main() {
    let stop_token = CancellationToken::new();
    // We don't use select() so we should be able to match the hard NOFILE limit.
    // NodeJS seems to do this as well.
    if let Err(e) = set_rlimit() {
        eprintln!("WARN: Failed to get/set NOFILE rlimit; error: {e}");
    }
    if let Err(e) = real_main(stop_token, false) {
        eprintln!("{e}");
        exit(1);
    }
}

#[cfg(windows)]
fn main() {
    let stop_token = CancellationToken::new();
    if svc_main(stop_token.clone()).is_err()
        && let Err(e) = real_main(stop_token, false)
    {
        eprintln!("{e}");
        exit(1);
    }
}

#[cfg(windows)]
fn svc_main(stop_tok: CancellationToken) -> Result<(), ()> {
    use windows_services::{Command, Service, State};

    let mut thread = None;
    Service::new()
        .can_stop()
        .run(move |service, msg| match msg {
            Command::Start if thread.is_none() => {
                thread = Some(unsafe {
                    std::thread::Builder::new()
                        .spawn_unchecked(move || {
                            real_main(stop_token.clone(), true)
                                .inspect_err(|_| service.set_state(State::Stopped))
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

fn real_main(stop_tok: CancellationToken, win_srv: bool) -> Result<(), MainErr> {
    let (config, webdata, cleanup_task) = get_config(stop_tok.clone())?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let shutdown_task = tasks::shutdown::shutdown(stop_tok.clone(), win_srv);
        let web_task = tasks::web::start_web(stop_tok, config, webdata);

        let (_, web_err) = tokio::try_join!(shutdown_task, web_task).unwrap();
        cleanup_task.join().unwrap();

        web_err.map_err(|e| e.into())
    })
}
