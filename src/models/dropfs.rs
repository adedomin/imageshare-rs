use std::path::{Path, PathBuf};

pub struct DropFsGuard<'a> {
    inner: Option<&'a Path>,
}

/// Build a path using a app generated filename and a base path.
pub fn build_with_base<T: AsRef<Path>>(path: T, fname: &str) -> PathBuf {
    path.as_ref().join(fname)
}

#[cfg(windows)]
pub mod windows {
    use std::{fs, path::PathBuf, sync::LazyLock, thread};

    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    pub static DEL_THREAD_HANDLE: LazyLock<UnboundedSender<PathBuf>> = LazyLock::new(|| {
        let (send, mut recv) = unbounded_channel();
        thread::spawn(move || {
            let mut buf = Vec::with_capacity(8);
            while recv.blocking_recv_many(&mut buf, 8) > 0 {
                buf.drain(..).for_each(|f| {
                    _ = fs::remove_file(f);
                })
            }
            eprintln!("[DEL_THREAD] Receiver closed, exiting.");
        });
        send
    });
}

#[cfg(windows)]
/// On Windows, removing files can block for some unreasonable time.
pub fn background_rm_file(del: PathBuf) {
    _ = windows::DEL_THREAD_HANDLE.send(del);
}

#[cfg(unix)]
/// On non-Windows, this *should* be reasonably fast unless your system is deeply cursed full of AV nonsense.
pub fn background_rm_file(del: PathBuf) {
    _ = std::fs::remove_file(del);
}

impl<'a> DropFsGuard<'a> {
    pub fn new<T: AsRef<Path> + 'a>(path: &'a T) -> Self {
        Self {
            inner: Some(path.as_ref()),
        }
    }

    pub fn defuse(mut self) {
        _ = self.inner.take();
    }
}

impl<'a> Drop for DropFsGuard<'a> {
    fn drop(&mut self) {
        if let Some(path) = self.inner {
            background_rm_file(path.to_path_buf());
        }
    }
}
