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
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
};

use quick_cache::{DefaultHashBuilder, Lifecycle, Weighter, sync::Cache};
use rand::{Rng as _, seq::SliceRandom as _};
use sqids::Sqids;

use crate::config::StorageSettings;

use crate::models::dropfs::{background_rm_file, build_with_base};

#[derive(Clone)]
pub struct FsWeighter;

pub type Key = String;
pub type Val = usize;

impl Weighter<Key, Val> for FsWeighter {
    fn weight(&self, _key: &Key, val: &Val) -> u64 {
        *val as u64
    }
}

#[derive(Clone)]
pub struct FsLifecycle(PathBuf);

impl FsLifecycle {
    pub fn new(p: PathBuf) -> Self {
        Self(p)
    }
}

impl Lifecycle<Key, Val> for FsLifecycle {
    type RequestState = ();

    fn on_evict(&self, _state: &mut Self::RequestState, key: Key, _val: Val) {
        let full = build_with_base(&self.0, &key);
        background_rm_file(full);
    }
}

pub struct StorageState {
    base: PathBuf,
    st_blksize: usize,
    siz: NonZeroUsize,
    stor: Option<Cache<String, usize, FsWeighter, DefaultHashBuilder, FsLifecycle>>,
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

    pub fn push(&self, fname: String, siz: usize) {
        if let Some(stor) = &self.stor {
            // files are *at least* st_blksize big.
            stor.insert(fname, siz.max(self.st_blksize));
        }
    }

    pub fn refresh(&self, fname: &str) -> bool {
        if let Some(stor) = &self.stor {
            stor.get(fname).is_some()
        } else {
            true
        }
    }

    fn prepopulate(&self) -> std::io::Result<()> {
        let read_dir = match std::fs::read_dir(&self.base) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&self.base)?;
                // nothing to read, unless we have some kind of TOCTOU situation, which would be bizarre.
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        if let Some(stor) = &self.stor {
            for file in read_dir {
                let dent = file?;
                let path = dent.path();
                if path.is_file() {
                    let siz = usize::try_from(dent.metadata()?.len())
                        .expect("Ridiculously huge file found.");
                    let fname = path
                        .strip_prefix(&self.base)
                        .expect("Came from self.base, not possible?");
                    let fname = fname.to_string_lossy();
                    stor.insert(fname.into_owned(), siz);
                }
            }
        }
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

// most filesystems allocate out in 4K blocks in 2026
const DEFAULT_MIN_SIZ: usize = 4096;

#[cfg(target_os = "linux")]
fn get_min_siz(path: &Path) -> usize {
    use std::{fs, os::linux::fs::MetadataExt as _};
    let Ok(meta) = fs::metadata(path) else {
        return DEFAULT_MIN_SIZ;
    };
    meta.st_blksize() as usize
}

// TODO: Windows.
#[cfg(not(target_os = "linux"))]
fn get_min_siz(_path: &Path) -> usize {
    DEFAULT_MIN_SIZ
}

impl<const T: usize> TryFrom<StorageSettings<T>> for StorageState {
    type Error = std::io::Error;

    fn try_from(value: StorageSettings<T>) -> Result<Self, Self::Error> {
        let stor = value.cnt.map(|v| v.get()).map(|cap| {
            // note, may hold less than cap.
            Cache::with(
                cap,
                (value.siz.get() * cap) as u64,
                FsWeighter,
                DefaultHashBuilder::default(),
                FsLifecycle::new(value.dir.clone()),
            )
        });

        let mut rng = rand::rng();
        let mut rand_alpha = sqids::DEFAULT_ALPHABET.chars().collect::<Vec<_>>();
        rand_alpha.shuffle(&mut rng);
        let idgen = Sqids::builder()
            .alphabet(rand_alpha)
            .build()
            .expect("Should not happen. Alphabet is from the crate, but shuffled.");

        // we check the blksiz for the root of our storage dir, in case another mount has another value.
        let st_blksize = get_min_siz(&value.dir);

        let s = Self {
            base: value.dir,
            st_blksize,
            siz: value.siz,
            stor,
            idgen,
            seqno: AtomicU64::new(0),
        };
        s.prepopulate()?;
        Ok(s)
    }
}
