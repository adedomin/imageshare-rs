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

use crate::tasks::cleanup::cleanup_shutdown;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
pub fn shutdown(stop_tok: CancellationToken, _win_srv: bool) -> JoinHandle<()> {
    use tokio::signal::{
        ctrl_c,
        unix::{self, SignalKind},
    };

    tokio::spawn(async move {
        let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = ctrl_c() => {}
            _ = sigterm.recv() => {}
            _ = stop_tok.cancelled() => {}
        }
        stop_tok.cancel();
        cleanup_shutdown();
        eprintln!("WARN: Shutting down.");
    })
}

#[cfg(windows)]
pub fn shutdown(stop_tok: CancellationToken, win_srv: bool) -> JoinHandle<()> {
    use tokio::signal::windows;

    tokio::spawn(async move {
        // only do something if we're not a windows service.
        if !win_srv {
            // conhost
            let mut ctrl_break = windows::ctrl_break().unwrap();
            let mut ctrl_c = windows::ctrl_c().unwrap();
            let mut ctrl_close = windows::ctrl_close().unwrap();
            let mut ctrl_logoff = windows::ctrl_logoff().unwrap();
            let mut ctrl_shutdown = windows::ctrl_shutdown().unwrap();
            tokio::select! {
                _ = ctrl_break.recv() => {}
                _ = ctrl_c.recv() => {}
                _ = ctrl_close.recv() => {}
                _ = ctrl_logoff.recv() => {}
                _ = ctrl_shutdown.recv() => {}
                _ = stop_tok.cancelled() => {}
            }
            stop_tok.cancel();
            cleanup_shutdown();
            eprintln!("WARN: Shutting down.");
        }
    })
}
