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
    io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
    time::SystemTime,
};

use rand::{Rng as _, seq::SliceRandom as _};
use sqids::Sqids;

use crate::config::StorageSettings;

pub struct StorageState {
    base: PathBuf,
    siz: NonZeroUsize,
    cap: usize,
    idgen: Sqids,
    seqno: AtomicU64,
}

impl StorageState {
    pub fn get_base(&self) -> &Path {
        &self.base
    }

    pub fn get_max_siz(&self) -> usize {
        self.siz.get()
    }

    pub fn get_max_cap(&self) -> usize {
        self.cap
    }

    pub fn prepopulate(&self, send: fn(PathBuf) -> bool) -> std::io::Result<()> {
        let read_dir = match std::fs::read_dir(&self.base) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&self.base)?;
                // nothing to read, unless we have some kind of TOCTOU situation, which would be bizarre.
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let mut files = read_dir
            .map(|dent| {
                dent.and_then(|d| {
                    let path = d.path();
                    let meta = d.metadata()?;
                    Ok((path, meta.modified()?))
                })
            })
            .collect::<io::Result<Vec<(PathBuf, SystemTime)>>>()?;
        // sort by mtime
        files.sort_unstable_by_key(|(_, time)| *time);
        files.into_iter().for_each(|(p, _)| _ = send(p));
        Ok(())
    }

    pub fn gen_new_fname(&self, ext: &'static str) -> String {
        for _ in 0..64 {
            let seq = self
                .seqno
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // pads out the id for low sequence numbers and adds minor random noise to it.
            let rand_junk = rand::rng().random::<u16>() as u64;
            // unlikely, but it could fail to generate an ID due to offensive words.
            if let Ok(id) = self.idgen.encode(&[seq, rand_junk]) {
                return format!("{id}.{ext}",);
            }
        }
        panic!("Failed to generate an ID after 64 attempts. Something is wrong.");
    }
}

impl<const T: usize> From<StorageSettings<T>> for StorageState {
    fn from(value: StorageSettings<T>) -> Self {
        let mut rng = rand::rng();
        let mut rand_alpha = sqids::DEFAULT_ALPHABET.chars().collect::<Vec<_>>();
        rand_alpha.shuffle(&mut rng);
        let idgen = Sqids::builder()
            .alphabet(rand_alpha)
            .build()
            .expect("Should not happen. Alphabet is from the crate, but shuffled.");

        Self {
            base: value.dir,
            siz: value.siz,
            cap: value.cnt.map(|c| c.get()).unwrap_or(0),
            idgen,
            seqno: AtomicU64::new(0),
        }
    }
}
