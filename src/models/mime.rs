pub const MIME: [(&[u8], usize, &str); 13] = [
    (&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], 0, "png"),
    (&[0xFF, 0xD8, 0xFF], 0, "jpg"),
    (
        &[
            0, 0, 0, 0x0C, 0x4A, 0x58, 0x4C, 0x20, 0x0D, 0x0A, 0x87, 0x0A,
        ],
        0,
        "jxl",
    ),
    (b"GIF87a", 0, "gif"),
    (b"GIF89a", 0, "gif"),
    (&[0x1A, 0x45, 0xDF, 0xA3], 0, "webm"),
    (b"ftypMSNV", 4, "mp4"),
    (b"ftypisom", 4, "mp4"),
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
