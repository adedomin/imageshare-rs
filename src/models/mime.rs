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
pub const MIME: [(&[u8], usize, &str); 15] = [
    (b"\x89PNG\r\n\x1A\n", 0, "png"),
    (b"\xFF\xD8\xFF", 0, "jpg"),
    (b"\xFF\n", 0, "jxl"),
    (b"\x00\x00\x00\x0CJXL \r\n\x87\n", 0, "jxl"),
    (b"GIF87a", 0, "gif"),
    (b"GIF89a", 0, "gif"),
    (b"\x1A\x45\xDF\xA3", 0, "webm"),
    (b"ftypMSNV", 4, "mp4"),
    (b"ftypisom", 4, "mp4"),
    (b"ftypmp42", 4, "mp4"),
    (b"WEBP", 8, "webp"),
    (b"ftypavif", 4, "avif"),
    (b"ftypheic", 4, "heic"),
    (b"ftypqt", 4, "mov"),
    (b"moov", 4, "mov"),
];

pub const BYTES_NEEDED: usize = {
    let mut i = 0;
    let mut max = usize::MIN;
    while i < MIME.len() {
        let (magic, off, _) = MIME[i];
        let siz = magic.len() + off;
        if max < siz {
            max = siz;
        }
        i += 1;
    }
    max
};

/// Attempt to detect an extention from arbitrary bytes.
pub fn detect_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < BYTES_NEEDED {
        return None;
    }
    MIME.iter()
        .find_map(|&(magic, off, ext)| bytes[off..BYTES_NEEDED].starts_with(magic).then_some(ext))
}

// fn test_file<T: AsRef<std::path::Path>>(p: T) -> Option<&'static str> {
//     let mut f = std::fs::File::open(p).ok()?;
//     let mut buf = vec![0; 64];
//     use std::io::Read;
//     f.read_exact(&mut buf).ok()?;
//     drop(f);
//     println!("{buf:?}");
//     get_ext(&buf)
// }
