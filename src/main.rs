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
    rt.block_on(async {
        let web = web::start_web(config, webdata);
        web.await.unwrap().unwrap()
    });
}
