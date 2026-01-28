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

use crate::config::get_config;
mod config;
mod middleware;
mod models;
mod web;

fn main() {
    let (config, webdata) = match get_config() {
        Ok(webdata) => webdata,
        Err(e) => {
            eprintln!("{e}\n\nusage: {} [ config.json ]", env!("CARGO_PKG_NAME"));
            exit(1);
        }
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .unwrap();
    if let Err(e) = rt.block_on(async {
        let web = web::start_web(config, webdata);
        web.await.unwrap()
    }) {
        eprintln!("ERR: {e}");
        exit(1);
    }
}
