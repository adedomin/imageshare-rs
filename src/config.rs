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
use std::{
    io::{self, ErrorKind},
    num::{NonZero, NonZeroUsize},
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    config::env_vars::{config, data, rt},
    models::{storage::StorageState, webdata::WebData},
    tasks::cleanup::{CleanupTaskHandle, cleanup_shutdown, cleanup_task, new_image, new_paste},
};

#[cfg(unix)]
mod env_vars {
    pub mod config {
        pub const BASE: &str = "CONFIGURATION_DIRECTORY";
        pub const USER: &str = "XDG_CONFIG_HOME";
        pub const FALLBACK: &str = "/etc";
    }
    pub mod data {
        pub const BASE: &str = "STATE_DIRECTORY";
        pub const USER: &str = "XDG_DATA_HOME";
        pub const FALLBACK: &str = "/var/lib";
    }
    pub mod rt {
        pub const BASE: &str = "RUNTIME_DIRECTORY";
        pub const USER: &str = "XDG_RUNTIME_DIR";
        pub const FALLBACK: &str = "/run";
    }
}

#[cfg(windows)]
mod env_vars {
    pub mod config {
        pub const BASE: &str = "IMAGESHARE_HOME";
        pub const USER: &str = "AppData";
        pub const FALLBACK: &str = r"C:\ProgramData";
    }
    pub mod data {
        pub use super::config::*;
    }
    pub mod rt {
        pub use super::config::*;
    }
}

/// It's assumed the package name is the "Above Path" in the XDG and fallback case.
fn find_systemd_or_xdg_path(systemd: &str, xdg: &str, fallback: &str, dest: &str) -> PathBuf {
    let mut base = std::env::var_os(systemd)
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(xdg).map(|p| {
                let mut p = PathBuf::from(p);
                p.push(env!("CARGO_PKG_NAME"));
                p
            })
        })
        .unwrap_or_else(|| {
            let mut p = PathBuf::from(fallback);
            p.push(env!("CARGO_PKG_NAME"));
            p
        });
    base.push(dest);
    base
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("I/O Error: {0}")]
    IoErr(#[from] io::Error),
    #[error("Failed to deserialize config: {0}")]
    DeserConfig(#[from] serde_json::Error),
    #[error("Config file not found at {0:?} - see example config below:\n\n{EXAMPLE_CONFIG}")]
    NoConfig(PathBuf),
}

const DEFAULT_DIR_BASE: [&str; 2] = ["i", "p"];
const DEFAULT_SIZ_LIM: [usize; 2] = [10485760 /* 10MiB */, 65536 /* 64KiB */];

fn siz_default<const T: usize>() -> NonZeroUsize {
    NonZeroUsize::new(DEFAULT_SIZ_LIM[T]).unwrap()
}

fn dir_default<const T: usize>() -> PathBuf {
    find_systemd_or_xdg_path(data::BASE, data::USER, data::FALLBACK, DEFAULT_DIR_BASE[T])
}

#[derive(Deserialize)]
pub struct StorageSettings<const T: usize> {
    #[serde(default = "siz_default::<T>")]
    pub siz: NonZeroUsize,
    pub cnt: Option<NonZeroUsize>,
    #[serde(default = "dir_default::<T>")]
    pub dir: PathBuf,
}

impl<const T: usize> Default for StorageSettings<T> {
    fn default() -> Self {
        Self {
            siz: siz_default::<T>(),
            cnt: None,
            dir: dir_default::<T>(),
        }
    }
}

#[derive(Deserialize, Clone)]
pub struct Ratelim {
    pub secs: Option<NonZero<u64>>,
    pub burst: Option<NonZero<u32>>,
    pub trust_headers: Option<bool>,
    pub bucket_size: Option<NonZero<usize>>,
}

// (8 * 16384)B = 128KiB
const DEFAULT_BUCKET: usize = 16384;

impl Ratelim {
    pub fn secs(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.secs.map(|nz| nz.get()).unwrap_or(30))
    }

    pub fn burst(&self) -> NonZero<u32> {
        self.burst.unwrap_or(NonZero::new(3u32).unwrap())
    }

    pub fn trust_headers(&self) -> bool {
        // Isn't really intended to be run without a revproxy, assume true if omitted.
        self.trust_headers.unwrap_or(true)
    }

    pub fn bucket_size(&self) -> usize {
        self.bucket_size
            .map(|nz| nz.get())
            .unwrap_or(DEFAULT_BUCKET)
    }
}

fn bind_default() -> String {
    "[::1]:8146".to_owned()
}

#[derive(Deserialize)]
pub struct Config {
    paste: Option<StorageSettings<1>>,
    image: Option<StorageSettings<0>>,
    pub ratelim: Option<Ratelim>,
    #[serde(default)]
    pub link_prefix: String,
    #[serde(default = "bind_default")]
    bind: String,
}

const PORT_ENV: [&str; 3] = ["HTTP_PLATFORM_PORT", "FUNCTIONS_CUSTOMHANDLER_PORT", "8146"];

impl Config {
    pub fn get_bind_addr(&self) -> String {
        if let Some(rtdir) = self.bind.strip_prefix("rt-dir:") {
            let socket_base = find_systemd_or_xdg_path(rt::BASE, rt::USER, rt::FALLBACK, rtdir);
            format!("unix:{}", socket_base.to_string_lossy())
        } else if let Some(inet) = self.bind.strip_suffix(":%PORT%") {
            let port = std::env::var(PORT_ENV[0])
                .or_else(|_| std::env::var(PORT_ENV[1]))
                .unwrap_or_else(|_| {
                    eprintln!(
                        "WARN: %HTTP_PLATFORM_PORT% could not be read! defaulting to {}",
                        PORT_ENV[2]
                    );
                    PORT_ENV[2].to_string()
                });
            format!("{inet}:{port}")
        } else {
            self.bind.clone()
        }
    }

    fn is_unix_listener(&self) -> bool {
        self.get_bind_addr().strip_prefix("unix:").is_some()
    }

    fn get_webdata_and_cleanup(
        &mut self,
        stop_tok: CancellationToken,
    ) -> Result<(Arc<WebData>, CleanupTaskHandle), ConfigError> {
        let paste = self.paste.take().unwrap_or_default();
        let paste = StorageState::from(paste);
        let image = self.image.take().unwrap_or_default();
        let image = StorageState::from(image);
        let task_handle = cleanup_task(stop_tok, paste.get_max_cap(), image.get_max_cap());
        if let Err(e) = || -> io::Result<()> {
            paste.prepopulate(new_paste)?;
            image.prepopulate(new_image)?;
            Ok(())
        }() {
            // kill thread
            cleanup_shutdown();
            task_handle.join().unwrap();
            return Err(e.into());
        }

        Ok((
            Arc::new(WebData {
                image,
                paste,
                link_prefix: self.link_prefix.clone(),
            }),
            task_handle,
        ))
    }
}

pub fn open_and_parse<T>(config_path: T) -> Result<Config, ConfigError>
where
    T: std::fmt::Debug + AsRef<Path>,
{
    match std::fs::File::open(&config_path) {
        Ok(file) => {
            let file = io::BufReader::new(file);
            Ok(serde_json::from_reader(file)?)
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            Err(ConfigError::NoConfig(config_path.as_ref().to_path_buf()))
        }
        Err(e) => Err(e.into()),
    }
}

pub fn get_config(
    stop_tok: CancellationToken,
) -> Result<(Config, Arc<WebData>, CleanupTaskHandle), ConfigError> {
    let config = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            find_systemd_or_xdg_path(config::BASE, config::USER, config::FALLBACK, "config.json")
        });
    let mut config = open_and_parse(config)?;
    // fixup ratelim config in unix socket case.
    if config.is_unix_listener()
        && let Some(ratelim) = config.ratelim.as_mut()
        && let Some(false) = ratelim.trust_headers
    {
        eprintln!("WARN: ratelim.trust_headers must be true when using a unix listener!");
        ratelim.trust_headers = Some(true);
    }
    let (webdata, cleanup) = config.get_webdata_and_cleanup(stop_tok)?;
    Ok((config, webdata, cleanup))
}

const EXAMPLE_CONFIG: &str = r###"
{ "image":
    { "//": "Max allowed image size in bytes. default 10MiB."
    , "siz": 10485760
    , "//": "Max number of files before deleting. default: unlimited."
    , "cnt": 100
    , "//": "Path to store images in, default uses ${STATE_DIRECTORY}/i or ${XDG_DATA_HOME}/${CARGO_PKG_NAME}/i"
    , "dir": "./uploads/i"
    }
, "paste":
    { "//": "Max allowed paste size. Note: pastes are buffered in memory to check for utf8-ness and simplicity."
    , "siz": 65536
    , "//": "Max number of files before deleting. default: unlimited."
    , "cnt": 10000
    , "//": "Path to store images in, default uses ${STATE_DIRECTORY}/p or ${XDG_DATA_HOME}/${CARGO_PKG_NAME}/p"
    , "dir": "./uploads/p"
    }
, "ratelim":
    { "//": "Number of seconds to restore one token."
    , "secs": 30
    , "burst": 3
    , "//": "UNIX:    Trust X-Real-IP in place of SocketAddr on Request."
    , "//": "Windows: Trust The last IP in X-Forwarded-For."
    , "trust_headers": true
    , "//": "Max number of IPs to track. Fixed size. default: 16384 (~128KiB of state)."
    , "bucket_size": 16384
    }
, "//": "the path prepended to upload results"
, "link_prefix": "http://localhost:8146"
, "bind": "127.0.0.1:8146"
, "//": "For IIS HttpPlatformHandler (also for UNIX, I suppose)"
, "//": "%PORT% is replaced with the envvar:"
, "//": "%HTTP_PLATFORM_PORT% or %FUNCTIONS_CUSTOMHANDLER_PORT%"
, "// bind": "127.0.0.1:%PORT%"
, "//": "or the following uds ones"
, "//": "NOTE: uds permissions are set to 0o666. Make sure parent is secure."
, "//bind": "unix:./socket/path/here/does/not/need/to/be/full.sock"
, "//": "rt-dir: protocol expands to unix:${RUNTIME_DIRECTORY}/"
, "//": "${RUNTIME_DIRECTORY} can be the literal environment variable, or ${XDG_RUNTIME_DIR}/${CARGO_PKG_NAME}"
, "//bind": "rt-dir:web.sock"
}
"###;
