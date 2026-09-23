use std::path::Path;

use crate::tasks::cleanup::background_rm_file;

pub struct DropFsGuard<'a> {
    inner: Option<&'a Path>,
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
