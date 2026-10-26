//! Content-addressed media store: image BYTES live on disk, SQLite keeps only
//! a short path. Replaces the old posture where avatars (`users.avatar_url`),
//! server icons (`servers.icon_url`) and message attachments
//! (`message_attachments.url`) stored raw `data:image/…;base64,` URLs inline —
//! which made every message list and realtime frame carry the image bytes.
//!
//! Layout: directory = env `MEDIA_DIR`, else a `media/` directory next to the
//! database file (`DISCORD_DB`). File name = lowercase-hex sha256 of the
//! bytes + extension from the REAL (magic-byte detected) format:
//! `/media/<sha256>.<png|jpg|gif|webp>`. Identical bytes hash identically, so
//! storing them twice reuses the file. Writes are atomic: a temp file in the
//! SAME directory (rename is not atomic across filesystems), then rename — a
//! reader never sees a half-written image.
//!
//! Upload validation (in order): size cap first, then magic-byte format
//! detection (PNG / JPEG / GIF / WebP only — SVG is deliberately NOT allowed
//! for user-content uploads), then the claimed mime must agree with the
//! detected format, and the image must actually DECODE under explicit limits
//! (max 8192x8192, bounded alloc) so a decompression bomb is rejected. This
//! closes the old bug class: the malformed test PNGs whose IHDR chunk length
//! was 11 or 12 instead of 13 passed the old prefix check and painted as
//! broken images everywhere the frame carried them.
//!
//! Serving lives in `src/bin/server.rs` (`GET /media/{name}`) and goes
//! through [`media_response`]. Access control is deliberately only the
//! unguessable sha256 name — a leaked URL is fetchable by anyone who has it.

use base64::engine::general_purpose::STANDARD;
use base64::engine::Engine;

use image::{guess_format, ImageFormat, ImageReader, Limits};

use sha2::{Digest, Sha256};

use std::io::Cursor;
use std::path::PathBuf;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use std::fs::{create_dir_all, exists, read, remove_file, rename, write};

/// One accepted upload format: image-crate format → file extension → mime.
/// SVG is NOT in this set (user-content uploads never become SVG on disk).
const FORMATS: &[(ImageFormat, &str, &str)] = &[
    (ImageFormat::Png, "png", "image/png"),
    (ImageFormat::Jpeg, "jpg", "image/jpeg"),
    (ImageFormat::Gif, "gif", "image/gif"),
    (ImageFormat::WebP, "webp", "image/webp"),
];

/// Decoding hard limits: reject a decompression bomb (a tiny file claiming an
/// enormous canvas) and oversized canvases before anything large is allocated.
const MAX_DECODE_DIM: u32 = 8192;
const MAX_DECODE_ALLOC: u64 = 64 * 1024 * 1024;

/// The web path prefix a stored file is addressed by.
const MEDIA_PREFIX: &str = "/media/";

/// Resolve the media directory for THIS process: `MEDIA_DIR` when set, else
/// `media/` next to the database file (`DISCORD_DB`, default `discord.db`).
/// Unit tests never write next to the crate: every test process shares one
/// directory under the system temp dir.
pub fn media_dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join("discord-server-test-media");
    }
    match std::env::var("MEDIA_DIR") {
        Ok(d) => return PathBuf::from(d),
        Err(_) => {}
    }
    let db = std::env::var("DISCORD_DB").unwrap_or_else(|_| "discord.db".to_string());
    let p = PathBuf::from(db);
    match p.parent() {
        Some(q) if !q.is_empty() => return PathBuf::from(q).join("media"),
        _ => {}
    }
    PathBuf::from("media")
}

/// Whether `name` is a storage-safe media file name: exactly
/// `^[0-9a-f]{64}\.(png|jpg|gif|webp)$`. Anything else — `..`, a path
/// separator, an uppercase hash, a foreign extension, an empty name — is
/// rejected, so a request for one never touches the filesystem.
pub fn media_filename_ok(name: &str) -> bool {
    let b = name.as_bytes();
    // 64 hash bytes + '.' + one of the four extensions (3..4 chars).
    if b.len() != 64 + 1 + 3 && b.len() != 64 + 1 + 4 {
        return false;
    }
    if b[64] != b'.' {
        return false;
    }
    let hash_ok = b[..64].iter().all(
        |c| (*c >= b'0' && *c <= b'9') || (*c >= b'a' && *c <= b'f')
    );
    if !hash_ok {
        return false;
    }
    let ext: &str = &name[65..];
    FORMATS.iter().any(|(_, e, _)| *e == ext)
}

/// The `data:image/…;base64,` grammar the client uploads: `data:<mime>;base64,`
/// with <mime> one of the accepted image mimes. Returns (mime, base64 body).
pub fn parse_data_image_url(value: &str) -> Option<(String, String)> {
    const PREFIX: &str = "data:image/";
    if !value.starts_with(PREFIX) {
        return None;
    }
    let rest = &value[PREFIX.len()..];
    let semi = match rest.find(';') {
        Some(i) => i,
        None => return None,
    };
    let mime = "image/".to_string() + &rest[..semi];
    if !FORMATS.iter().any(|(_, _, m)| *m == mime) {
        return None;
    }
    if !rest[semi..].starts_with(";base64,") {
        return None;
    }
    Some((mime, rest[semi + ";base64,".len()..].to_string()))
}

/// Decode a `data:image/…;base64,` URL into (mime, bytes): `None` when the
/// prefix is malformed (not an accepted image mime) or the body does not
/// decode.
pub fn decode_data_image_url(value: &str) -> Option<(String, Vec<u8>)> {
    let (mime, body) = match parse_data_image_url(value) {
        Some(p) => p,
        None => return None,
    };
    match STANDARD.decode(body) {
        Ok(bytes) => Some((mime, bytes)),
        Err(_) => None,
    }
}

/// Content identity of bytes: lowercase-hex sha256 (64 chars).
pub fn content_hash(bytes: &[u8]) -> String {
    let mut d = Sha256::new();
    d.update(bytes);
    let out = d.finalize();
    const HEX: &[char] = &[
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    let mut s = String::with_capacity(64);
    for b in out.as_slice() {
        s.push(HEX[(b >> 4) as usize]);
        s.push(HEX[(b & 0xf) as usize]);
    }
    s
}

// ── storing ─────────────────────────────────────────────────────────────────

/// Atomic content-addressed write into `dir` (created on demand): name
/// `<sha256>.<ext>`, TEMP FILE in the SAME directory (rename is not atomic
/// across filesystems) + rename. An existing file for the same bytes is
/// reused untouched. Returns `/media/<name>`.
fn write_content(dir: PathBuf, bytes: &[u8], ext: &str) -> Result<String, String> {
    let name = content_hash(bytes) + "." + ext;
    let web_path = MEDIA_PREFIX.to_string() + &name;
    match create_dir_all(&dir) {
        Err(e) => return Err(format!("cannot create media dir {dir:?}: {e}")),
        Ok(_) => {}
    }
    let target = dir.join(&name);
    match exists(&target) {
        Ok(true) => return Ok(web_path),
        Ok(_) => {}
        Err(_) => {}
    }
    static WRITER: AtomicU64 = AtomicU64::new(0);
    let n = WRITER.fetch_add(1, Ordering::Relaxed);
    // pid + counter: concurrent writers (threads or processes) never share a temp file.
    let tmp = dir.join(format!(".{name}.tmp-{}-{n}", std::process::id()));
    match write(&tmp, bytes) {
        Err(_) => return Err(format!("cannot create media temp file in {dir:?}")),
        Ok(_) => {}
    }
    match rename(&tmp, &target) {
        Err(e) => {
            let _ = remove_file(&tmp);
            return Err(format!("media rename failed: {e}"));
        }
        Ok(_) => {}
    }
    Ok(web_path)
}

/// Decode-under-limits validation: the bytes must parse as one of the four
/// accepted formats AND actually decode (8192x8192 cap, bounded alloc). This
/// is what rejects the malformed-PNG class (an IHDR chunk whose declared
/// length is 11 or 12 instead of 13 fails the chunk-table parse) and
/// decompression bombs. Pixels are discarded; only the outcome matters.
fn validate_decodable(bytes: &[u8]) -> Result<(), String> {
    let mut reader = match ImageReader::new(Cursor::new(bytes.to_vec())).with_guessed_format() {
        Ok(r) => r,
        Err(e) => return Err(format!("image stream could not be opened: {e}")),
    };
    let mut lim = Limits::default();
    lim.max_image_width = Some(MAX_DECODE_DIM);
    lim.max_image_height = Some(MAX_DECODE_DIM);
    lim.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(lim);
    match reader.decode() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("image does not decode: {e}")),
    }
}

/// The upload path: size cap first, then magic-byte format detection
/// (PNG/JPEG/GIF/WebP only — SVG and everything else rejected), then the
/// CLAIMED mime must match the detected format, then the image must
/// actually decode. Returns the media path `/media/<sha256>.<ext>`.
pub fn store(bytes: &[u8], claimed_mime: &str, max_bytes: usize) -> Result<String, String> {
    store_in(media_dir(), bytes, claimed_mime, max_bytes)
}

/// [`store`] against an explicit media directory (tests + the migration pass).
pub fn store_in(
    dir: PathBuf,
    bytes: &[u8],
    claimed_mime: &str,
    max_bytes: usize,
) -> Result<String, String> {
    if bytes.len() > max_bytes {
        let got = bytes.len();
        return Err(format!("image too large (max {max_bytes} bytes, got {got})"));
    }
    let detected = match guess_format(bytes) {
        Ok(f) => f,
        Err(_) => return Err("image format not recognized (not PNG/JPEG/GIF/WebP)".into()),
    };
    let (ext, mime) = match FORMATS.iter().find(|(f, _, _)| *f == detected) {
        Some((_, e, m)) => (e, m.to_string()),
        None => return Err("image format not allowed".into()),
    };
    if mime != claimed_mime {
        return Err(format!(
            "claimed mime '{claimed_mime}' does not match the detected format ('{mime}')"
        ));
    }
    match validate_decodable(bytes) {
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    write_content(dir, bytes, ext)
}

/// Pixel size `(width, height)` of a stored image, read from its file header
/// (no full decode). The client sizes an attachment's box from it. Files are
/// content-addressed, so a path's bytes never change and each path is read
/// once, then cached. `None` for a legacy inline `data:` value, a name that
/// fails [`media_filename_ok`], or a missing or unreadable file.
pub fn dimensions(url: &str) -> Option<(u32, u32)> {
    dimensions_in(media_dir(), url)
}

/// [`dimensions`] against an explicit media directory (tests).
pub fn dimensions_in(dir: PathBuf, url: &str) -> Option<(u32, u32)> {
    let name = url.strip_prefix(MEDIA_PREFIX)?;
    if !media_filename_ok(name) {
        return None;
    }
    let path = dir.join(name);
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, (u32, u32)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(dims) = cache.lock().ok()?.get(&path) {
        return Some(*dims);
    }
    let dims = ImageReader::open(&path).ok()?.with_guessed_format().ok()?.into_dimensions().ok()?;
    cache.lock().ok()?.insert(path, dims);
    Some(dims)
}

/// Store bytes with NO format validation, extension from the (already
/// recognized) mime — the one-time legacy pass: old inline values are
/// preserved byte-for-byte even when malformed, so no user data is lost.
pub fn store_legacy_in(dir: PathBuf, bytes: &[u8], mime: &str) -> Option<String> {
    let ext = match FORMATS.iter().find(|(_, _e, m)| *m == mime) {
        Some((_, e, _)) => e,
        None => return None,
    };
    match write_content(dir, bytes, ext) {
        Ok(path) => Some(path),
        Err(_) => None,
    }
}

// ── one-time legacy migration ──────────────────────────────────────────────

/// The columns that held inline images before the media store:
/// (table, column). Each table has an INTEGER `id` primary key.
const LEGACY_COLUMNS: &[(&str, &str)] = &[
    ("users", "avatar_url"),
    ("servers", "icon_url"),
    ("message_attachments", "url"),
];

/// Idempotent one-time pass over the existing database: every still-inline
/// `data:` value in `users.avatar_url`, `servers.icon_url` and
/// `message_attachments.url` moves its decoded bytes into the media store AS-IS
/// (no format validation — a malformed legacy image must not be dropped) and
/// gets its column rewritten to the `/media/…` path. A value that cannot be
/// converted (base64 that does not decode, or a mime outside the four
/// formats) is LEFT UNCHANGED and counted, so the pass never silently destroys
/// data. Rows already holding a `/media/` path are not selected, so a restart
/// does nothing. Returns (converted, left unchanged).
pub fn migrate_existing(db: &crate::db::Db) -> (usize, usize) {
    migrate_rows(db, media_dir())
}

fn migrate_rows(db: &crate::db::Db, dir: PathBuf) -> (usize, usize) {
    let mut converted = 0;
    let mut unconverted = 0;
    let Ok(c) = db.lock() else {
        return (converted, unconverted);
    };
    for (table, column) in LEGACY_COLUMNS {
        let rows: Vec<(i64, String)> = {
            let sql = format!("SELECT id, {column} FROM {table} WHERE {column} LIKE 'data:%'");
            let Ok(mut stmt) = c.prepare(&sql) else { continue };
            let Ok(rows) = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))) else { continue };
            rows.filter_map(Result::ok).collect()
        };
        let update = format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2");
        for (id, value) in rows {
            match migrate_data_value(dir.clone(), &value) {
                Some(path) if c.execute(&update, rusqlite::params![path, id]).is_ok() => {
                    converted += 1
                }
                _ => unconverted += 1,
            }
        }
    }
    (converted, unconverted)
}

/// Convert ONE inline value: `Some(path)` when it is a `data:image/…;base64,`
/// value whose body decodes (bytes stored AS-IS, extension from the claimed
/// mime); `None` for a value that is NOT an inline image (already a path —
/// skip) or whose base64 body fails to decode (leave unchanged, counted).
pub fn migrate_data_value(dir: PathBuf, value: &str) -> Option<String> {
    let (mime, body) = parse_data_image_url(value)?;
    let bytes = STANDARD.decode(body).ok()?;
    store_legacy_in(dir, &bytes, &mime)
}

// ── serving ─────────────────────────────────────────────────────────────────

/// Read a media file for serving: (content-type from the EXTENSION — the
/// stored extension is the format truth, never a client-declared mime — bytes)
/// or `None` = 404 (name not storage-safe, or file unreadable).
pub fn media_response(dir: PathBuf, name: &str) -> Option<(String, Vec<u8>)> {
    if !media_filename_ok(name) {
        return None;
    }
    let buf = read(dir.join(name)).ok()?;
    let ext = &name[65..];
    let (_, _, mime) = FORMATS.iter().find(|(_, e, _)| *e == ext)?;
    Some((mime.to_string(), buf))
}

/// The HTTP response for `GET /media/{name}`: the file with exactly one
/// `Content-Type`, an immutable year-long cache (the name IS the content
/// hash) and `nosniff`; 404 for anything [`media_response`] refuses.
pub fn http_response(dir: PathBuf, name: &str) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    match media_response(dir, name) {
        None => StatusCode::NOT_FOUND.into_response(),
        Some((mime, bytes)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            ],
            bytes,
        )
            .into_response(),
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // The malformed-PNG class: IHDR chunk length 11 / 12 instead of the
    // canonical 13 — these used to pass the old prefix check and paint as
    // broken images everywhere. The new validator must reject them.
    const BAD11: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 11, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220, 0, 0, 0, 9, 112, 72, 89, 115, 0, 0, 0, 1, 0, 0, 0, 1, 0, 79, 37, 196, 214, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 144, 141, 90, 142, 21, 49, 12, 45, 9, 0, 205, 221, 71, 129, 78, 1, 50, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    const BAD12: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 12, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220, 0, 0, 0, 9, 112, 72, 89, 115, 0, 0, 0, 1, 0, 0, 0, 1, 0, 79, 37, 196, 214, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 144, 141, 90, 142, 21, 49, 12, 45, 9, 0, 205, 221, 71, 129, 78, 1, 50, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    const PNG: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 8, 0, 0, 0, 8, 8, 2, 0, 0, 0, 75, 109, 41, 220, 0, 0, 0, 9, 112, 72, 89, 115, 0, 0, 0, 1, 0, 0, 0, 1, 0, 79, 37, 196, 214, 0, 0, 0, 17, 73, 68, 65, 84, 120, 156, 99, 144, 141, 90, 142, 21, 49, 12, 45, 9, 0, 205, 221, 71, 129, 78, 1, 50, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
    const JPG: &[u8] = &[255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 2, 0, 0, 1, 0, 1, 0, 0, 255, 254, 0, 16, 76, 97, 118, 99, 54, 49, 46, 49, 57, 46, 49, 48, 49, 0, 255, 219, 0, 67, 0, 8, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 7, 7, 7, 6, 6, 7, 7, 8, 8, 8, 8, 9, 9, 9, 8, 8, 8, 8, 9, 9, 10, 10, 10, 12, 12, 11, 11, 14, 14, 14, 17, 17, 20, 255, 196, 0, 75, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 16, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 192, 0, 17, 8, 0, 8, 0, 8, 3, 1, 34, 0, 2, 17, 0, 3, 17, 0, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 145, 0, 196, 109, 255, 217];
    const WEBP: &[u8] = &[82, 73, 70, 70, 56, 0, 0, 0, 87, 69, 66, 80, 86, 80, 56, 32, 44, 0, 0, 0, 208, 1, 0, 157, 1, 42, 8, 0, 8, 0, 2, 0, 52, 37, 160, 2, 116, 186, 1, 248, 0, 3, 176, 0, 254, 240, 83, 31, 255, 44, 249, 252, 54, 117, 57, 255, 194, 175, 141, 200, 88, 80, 0, 0];
    /// A 1x1 GIF89a.
    const GIF_B64: &str = "R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";
    const CAP: usize = 256 * 1024;

    fn gif() -> Vec<u8> {
        STANDARD.decode(GIF_B64).unwrap()
    }

    fn data_url(mime: &str, bytes: &[u8]) -> String {
        format!("data:{mime};base64,{}", STANDARD.encode(bytes))
    }

    #[test]
    fn each_accepted_format_stores_under_its_real_extension() {
        let dir = tempfile::tempdir().unwrap();
        for (bytes, mime, ext) in [
            (PNG.to_vec(), "image/png", "png"),
            (JPG.to_vec(), "image/jpeg", "jpg"),
            (gif(), "image/gif", "gif"),
            (WEBP.to_vec(), "image/webp", "webp"),
        ] {
            let path = store_in(dir.path().into(), &bytes, mime, CAP).expect(mime);
            let name = path.strip_prefix(MEDIA_PREFIX).unwrap();
            assert!(media_filename_ok(name), "{path}");
            assert!(name.ends_with(&format!(".{ext}")), "{path}");
            assert_eq!(name[..64], content_hash(&bytes));
            assert_eq!(std::fs::read(dir.path().join(name)).unwrap(), bytes);
        }
    }

    #[test]
    fn dimensions_come_from_the_stored_file() {
        use image::ImageEncoder;
        // Not square, so a swapped width/height would show.
        let mut wide = Vec::new();
        image::codecs::png::PngEncoder::new(&mut wide)
            .write_image(&vec![0u8; 12 * 5], 12, 5, image::ExtendedColorType::L8)
            .map(|_| ())
            .unwrap_or_else(|e| panic!("encode: {e}"));
        let dir = tempfile::tempdir().unwrap();
        let png = store_in(dir.path().into(), &wide, "image/png", CAP).unwrap();
        assert_eq!(dimensions_in(dir.path().into(), &png), Some((12, 5)));
        // Cached: still answered after the file is gone.
        std::fs::remove_file(dir.path().join(&png[MEDIA_PREFIX.len()..])).unwrap();
        assert_eq!(dimensions_in(dir.path().into(), &png), Some((12, 5)));
        let jpg = store_in(dir.path().into(), JPG, "image/jpeg", CAP).unwrap();
        assert_eq!(dimensions_in(dir.path().into(), &jpg), Some((8, 8)));
        assert_eq!(dimensions_in(dir.path().into(), &data_url("image/png", PNG)), None);
        assert_eq!(dimensions_in(dir.path().into(), "/media/../discord.db"), None);
        let missing = format!("{MEDIA_PREFIX}{}.png", "0".repeat(64));
        assert_eq!(dimensions_in(dir.path().into(), &missing), None);
    }

    #[test]
    fn same_bytes_reuse_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = store_in(dir.path().into(), PNG, "image/png", CAP).unwrap();
        let b = store_in(dir.path().into(), PNG, "image/png", CAP).unwrap();
        assert_eq!(a, b);
        // One file, and no temp files left behind.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn malformed_ihdr_pngs_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        for bad in [BAD11, BAD12] {
            let err = store_in(dir.path().into(), bad, "image/png", CAP).unwrap_err();
            assert!(err.contains("does not decode"), "{err}");
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0, "nothing stored");
    }

    #[test]
    fn claimed_mime_must_match_the_detected_format() {
        let dir = tempfile::tempdir().unwrap();
        let err = store_in(dir.path().into(), PNG, "image/jpeg", CAP).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }

    #[test]
    fn svg_and_unknown_bytes_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#;
        for mime in ["image/svg+xml", "image/png"] {
            let err = store_in(dir.path().into(), svg, mime, CAP).unwrap_err();
            assert!(err.contains("not recognized"), "{err}");
        }
        // The upload grammar refuses an SVG data URL before any bytes are read.
        assert!(parse_data_image_url(&data_url("image/svg+xml", svg)).is_none());
    }

    #[test]
    fn oversize_is_rejected_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let err = store_in(dir.path().into(), JPG, "image/jpeg", JPG.len() - 1).unwrap_err();
        assert!(err.contains("too large"), "{err}");
        store_in(dir.path().into(), JPG, "image/jpeg", JPG.len()).expect("exactly at cap");
    }

    #[test]
    fn canvas_over_the_decode_limit_is_rejected() {
        use image::ImageEncoder;
        // A tiny, well-formed PNG that declares a 9000-pixel-wide canvas.
        let mut wide = Vec::new();
        image::codecs::png::PngEncoder::new(&mut wide)
            .write_image(&vec![0u8; 9000], 9000, 1, image::ExtendedColorType::L8)
            .map(|_| ())
            .unwrap_or_else(|e| panic!("encode: {e}"));
        let dir = tempfile::tempdir().unwrap();
        let err = store_in(dir.path().into(), &wide, "image/png", CAP).unwrap_err();
        assert!(err.contains("does not decode"), "{err}");
    }

    #[test]
    fn route_serves_stored_files_with_one_content_type() {
        use axum::http::{header, StatusCode};
        let dir = tempfile::tempdir().unwrap();
        let path = store_in(dir.path().into(), PNG, "image/png", CAP).unwrap();
        let name = path.strip_prefix(MEDIA_PREFIX).unwrap();

        let ok = http_response(dir.path().into(), name);
        assert_eq!(ok.status(), StatusCode::OK);
        let h = ok.headers();
        assert_eq!(h.get_all(header::CONTENT_TYPE).iter().count(), 1);
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert_eq!(h[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let hash = &name[..64];
        for bad in [
            "..".to_string(),
            "../t.db".to_string(),
            format!("../{name}"),
            format!("{}.png", hash.to_uppercase()),
            format!("{hash}.svg"),
            format!("{hash}.PNG"),
            format!("{hash}.png/"),
            format!("{}.png", &hash[..63]),
            // Well-formed, but never stored.
            format!("{}.png", "0".repeat(64)),
        ] {
            let res = http_response(dir.path().into(), &bad);
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{bad}");
        }
    }

    #[test]
    fn legacy_pass_converts_inline_values_as_is_and_is_idempotent() {
        use std::path::Path;
        let tmp = tempfile::tempdir().unwrap();
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let db = crate::db::Db::migrate(&tmp.path().join("t.db"), &migrations).unwrap();
        let malformed = data_url("image/png", BAD11);
        let good = data_url("image/webp", WEBP);
        db.lock()
            .unwrap()
            .execute(
                "INSERT INTO users (id, username, email, created_at, avatar_url) VALUES
                    (1,'a','a@x',1,?1),(2,'b','b@x',1,'data:image/png;base64,!!!'),
                    (3,'c','c@x',1,'/media/kept.png'),(4,'d','d@x',1,NULL)",
                [&malformed],
            )
            .unwrap();
        db.lock()
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO servers (id, name, owner_id, icon_url, created_at) VALUES (1,'S',1,'{good}',1);
                 INSERT INTO channels (id, server_id, name, kind, created_at) VALUES (1,1,'c','text',1);
                 INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES (1,1,1,'',1);
                 INSERT INTO message_attachments (message_id, filename, mime, url, created_at)
                    VALUES (1,'x.png','image/png','{malformed}',1);"
            ))
            .unwrap();

        let media = tmp.path().join("media");
        assert_eq!(migrate_rows(&db, media.clone()), (3, 1));
        // Restarting converts nothing new; the undecodable value is counted again.
        assert_eq!(migrate_rows(&db, media.clone()), (0, 1));

        let c = db.lock().unwrap();
        let col = |sql: &str| -> Option<String> { c.query_row(sql, [], |r| r.get(0)).unwrap() };
        let avatar = col("SELECT avatar_url FROM users WHERE id = 1").unwrap();
        let icon = col("SELECT icon_url FROM servers WHERE id = 1").unwrap();
        let att = col("SELECT url FROM message_attachments").unwrap();
        // The malformed avatar and attachment are the same bytes: one file,
        // stored AS-IS (no validation on this path).
        assert_eq!(avatar, att);
        let file = |p: &str| std::fs::read(media.join(p.strip_prefix(MEDIA_PREFIX).unwrap())).unwrap();
        assert_eq!(file(&avatar), BAD11);
        assert_eq!(file(&icon), WEBP);
        assert!(icon.ends_with(".webp"));
        assert_eq!(col("SELECT avatar_url FROM users WHERE id = 2").unwrap(), "data:image/png;base64,!!!");
        assert_eq!(col("SELECT avatar_url FROM users WHERE id = 3").unwrap(), "/media/kept.png");
        assert_eq!(col("SELECT avatar_url FROM users WHERE id = 4"), None);
    }
}
