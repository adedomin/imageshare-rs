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

use std::path::PathBuf;

#[allow(unused)]
#[derive(Debug, thiserror::Error)]
pub enum UdsErr {
    #[error("Could not bind socket: {0:?} -- Reason: {1}")]
    UknkUdsBind(PathBuf, std::io::Error),
    #[error("Could not create destion for socket: {0:?} -- Reason: {1}")]
    CreateParents(PathBuf, std::io::Error),
    #[error("Could not create destion for socket: {0:?} -- Reason: no parents?!")]
    NoParents(PathBuf),
    #[error("Could not remove stale socket: {0:?} -- Reason: {1}")]
    RemoveStale(PathBuf, std::io::Error),
    #[error("Fail looping making {0:?}")]
    Loop(PathBuf),
    #[error("Failed to set 0o0660 mode for socket: {0:?} -- Reason: {1}")]
    ChmodUds(PathBuf, std::io::Error),
    #[error("Windows; while Windows supports UDS, tokio and stdlib do not.")]
    Windows,
}

#[cfg(unix)]
pub mod unix {
    use std::{
        fs::{Permissions, create_dir_all, remove_file, set_permissions},
        io::ErrorKind,
        os::unix::fs::PermissionsExt as _,
        path::Path,
    };

    use tokio::net::{UnixListener, UnixStream};

    use super::UdsErr;

    // TODO: consider abstract sockets?
    pub async fn listen_uds(unix: &Path) -> Result<tokio::net::UnixListener, UdsErr> {
        let mut loop_ctr = 0;
        loop {
            if loop_ctr == 2 {
                return Err(UdsErr::Loop(unix.to_path_buf()));
            }
            loop_ctr += 1;
            match UnixListener::bind(unix) {
                Ok(uds) => {
                    // rely on parent dir's mode, ACL or other security to restrict access.
                    set_permissions(unix, Permissions::from_mode(0o0666))
                        .map_err(|e| UdsErr::ChmodUds(unix.to_path_buf(), e))?;
                    break Ok(uds);
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    if let Some(parent) = unix.parent() {
                        create_dir_all(parent)
                            .map_err(|e| UdsErr::CreateParents(unix.to_path_buf(), e))?;
                    } else {
                        return Err(UdsErr::NoParents(unix.to_path_buf()));
                    }
                }
                Err(e) if e.kind() == ErrorKind::AddrInUse => {
                    // check if someone is listening.
                    match UnixStream::connect(&unix).await {
                        Ok(_) => return Err(UdsErr::RemoveStale(unix.to_path_buf(), e)),
                        Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                            remove_file(unix)
                                .map_err(|e| UdsErr::RemoveStale(unix.to_path_buf(), e))?;
                        }
                        Err(e) => {
                            return Err(UdsErr::UknkUdsBind(unix.to_path_buf(), e));
                        }
                    }
                }
                Err(e) => return Err(UdsErr::UknkUdsBind(unix.to_path_buf(), e)),
            }
        }
    }
}
