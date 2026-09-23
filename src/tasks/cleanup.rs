use std::{
    collections::VecDeque, fs::remove_file, path::PathBuf, sync::OnceLock, thread::JoinHandle,
};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
enum Actions {
    Paste(PathBuf),
    Image(PathBuf),
    Delete(PathBuf),
    Shutdown,
}

static SEND_TO_TASK: OnceLock<mpsc::UnboundedSender<Actions>> = OnceLock::new();

pub fn background_rm_file(del: PathBuf) {
    let sender = SEND_TO_TASK.get().expect("sender not initialized!");
    if let Err(e) = sender.send(Actions::Delete(del)) {
        eprintln!("[SEND TASK] Error: {e}");
    }
}

pub fn new_paste(p: PathBuf) -> bool {
    let sender = SEND_TO_TASK.get().expect("sender not initialized!");
    sender.send(Actions::Paste(p)).is_ok()
}

pub fn new_image(p: PathBuf) -> bool {
    let sender = SEND_TO_TASK.get().expect("sender not initialized!");
    sender.send(Actions::Image(p)).is_ok()
}

pub fn cleanup_shutdown() {
    let sender = SEND_TO_TASK.get().expect("sender not initialized!");
    sender.send(Actions::Shutdown).unwrap();
}

fn push_inner(stor: &mut VecDeque<PathBuf>, new_path: PathBuf) -> Option<PathBuf> {
    // unlimited
    if stor.capacity() == 0 {
        return None;
    }

    if stor.len() == stor.capacity() {
        let ret = stor.pop_back();
        stor.push_front(new_path);
        ret
    } else {
        stor.push_front(new_path);
        None
    }
}

pub type CleanupTaskHandle = JoinHandle<()>;

pub fn cleanup_task(
    stop_tok: CancellationToken,
    paste_siz: usize,
    image_siz: usize,
) -> CleanupTaskHandle {
    let (send, mut recv) = mpsc::unbounded_channel();
    SEND_TO_TASK.set(send).unwrap();
    std::thread::spawn(move || {
        // if cleanup thread stops unexpectedly, kill everything.
        let _stop_drop = stop_tok.drop_guard();
        let mut paste_store = VecDeque::with_capacity(paste_siz);
        let mut image_store = VecDeque::with_capacity(image_siz);
        'out: while let Some(action) = recv.blocking_recv() {
            if let Some(path) = match action {
                Actions::Paste(p) => push_inner(&mut paste_store, p),
                Actions::Image(p) => push_inner(&mut image_store, p),
                Actions::Delete(p) => Some(p),
                Actions::Shutdown => break 'out,
            } {
                _ = remove_file(path);
            }
        }
    })
}
