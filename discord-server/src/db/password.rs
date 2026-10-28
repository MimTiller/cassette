// ── password KDF ───────────────────────────────────────────────────────────
//
// Passwords are stored as a self-describing PBKDF2-HMAC-SHA256 secret:
//   pbkdf2-sha256$<iterations>$<salt_b64>$<dk_b64>
// The `pbkdf2` crate isn't in the local registry, so we hand-roll it over the
// vendored `hmac` + `sha2` crates. `rand` supplies the per-password salt.

#[cfg(feature = "server")]
use hmac::{Hmac, Mac};
#[cfg(feature = "server")]
use sha2::Sha256;

#[cfg(feature = "server")]
type HmacSha256 = Hmac<Sha256>;

/// PBKDF2 iterations for new password hashes (OWASP's current figure for
/// PBKDF2-HMAC-SHA256). Each stored hash records its own count, so older
/// 100k hashes still verify and are upgraded at the next successful login
/// (`password_needs_rehash`).
#[cfg(feature = "server")]
const PBKDF2_ITERATIONS: u32 = 600_000;

/// Derive a key from `password` using PBKDF2-HMAC-SHA256.
///
/// HMAC's keyed inner and outer SHA-256 states are computed once; each of
/// the (600k) rounds is then exactly two single-block compressions through
/// `sha2::compress256`. That function is not generic, so it runs at the
/// `sha2` crate's own optimization level (raised for dev builds in
/// Cargo.toml) instead of this crate's unoptimized debug build, which keeps a
/// login fast on the dev servers.
#[cfg(feature = "server")]
fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32, dk_len: usize) -> Vec<u8> {
    use sha2::digest::generic_array::GenericArray;
    use sha2::Digest;

    const IV: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        sha2::compress256(state, &[*GenericArray::from_slice(block)]);
    }
    fn to_bytes(state: &[u32; 8]) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (chunk, word) in out.chunks_exact_mut(4).zip(state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }
    // The state after absorbing one 64-byte pad block (key XOR 0x36/0x5c).
    let mut key = [0u8; 64];
    if password.len() > 64 {
        key[..32].copy_from_slice(&Sha256::digest(password));
    } else {
        key[..password.len()].copy_from_slice(password);
    }
    let keyed_state = |pad: u8| {
        let mut block = [0u8; 64];
        for (b, k) in block.iter_mut().zip(key.iter()) {
            *b = k ^ pad;
        }
        let mut state = IV;
        compress(&mut state, &block);
        state
    };
    let (inner, outer) = (keyed_state(0x36), keyed_state(0x5c));
    // HMAC of a 32-byte message: one block of message + SHA-256 padding for a
    // total length of 64 + 32 bytes (768 bits), through each keyed state.
    let mut block = [0u8; 64];
    block[32] = 0x80;
    block[62..].copy_from_slice(&768u16.to_be_bytes());
    let mut hmac32 = |msg: &[u8; 32]| {
        block[..32].copy_from_slice(msg);
        let mut s = inner;
        compress(&mut s, &block);
        block[..32].copy_from_slice(&to_bytes(&s));
        let mut s = outer;
        compress(&mut s, &block);
        to_bytes(&s)
    };

    let mac_len = 32usize;
    let blocks = dk_len.div_ceil(mac_len);
    let mut out = Vec::with_capacity(blocks * mac_len);
    for index in 1..=blocks as u32 {
        // U1 = PRF(password, salt || INT_32_BE(index)): arbitrary length, so
        // it goes through the generic HMAC once.
        let mut mac = <HmacSha256 as Mac>::new_from_slice(password).expect("hmac key any size");
        mac.update(salt);
        mac.update(&index.to_be_bytes());
        let mut u: [u8; 32] = mac.finalize().into_bytes().into();
        let mut result = u;
        for _ in 1..iterations {
            u = hmac32(&u);
            for (r, x) in result.iter_mut().zip(u.iter()) {
                *r ^= *x;
            }
        }
        out.extend_from_slice(&result);
    }
    out.truncate(dk_len);
    out
}

fn b64encode(bytes: &[u8]) -> String {
    // Minimal standard base64 (no `base64` crate in the local registry).
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0) as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        s.push(A[(n >> 18 & 63) as usize] as char);
        s.push(A[(n >> 12 & 63) as usize] as char);
        s.push(if chunk.len() > 1 { A[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if chunk.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
    }
    s
}

fn b64decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let chars: Vec<u8> = s.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(chars.len() * 3 / 4);
    for chunk in chars.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let full = chunk.len().min(4);
        for i in 0..full - 1 {
            out.push(((n >> (16 - 8 * i)) & 0xff) as u8);
        }
    }
    Some(out)
}

/// Hash `password` into the stored `pbkdf2-sha256$iter$salt$dk` form.
#[cfg(feature = "server")]
pub fn hash_password(password: &str) -> String {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let dk = pbkdf2(password.as_bytes(), &salt, PBKDF2_ITERATIONS, 32);
    format!("pbkdf2-sha256${}${}${}", PBKDF2_ITERATIONS, b64encode(&salt), b64encode(&dk))
}

/// Whether `stored` was hashed with fewer iterations than new hashes use
/// (or is unreadable), so a successful login should store a fresh hash.
#[cfg(feature = "server")]
pub fn password_needs_rehash(stored: &str) -> bool {
    stored
        .split('$')
        .nth(1)
        .and_then(|i| i.parse::<u32>().ok())
        .map_or(true, |i| i < PBKDF2_ITERATIONS)
}

/// Spend the same time as checking a real password, for a username that
/// doesn't exist, so a login's timing doesn't reveal which accounts exist.
#[cfg(feature = "server")]
pub fn burn_password_check(password: &str) {
    let _ = pbkdf2(password.as_bytes(), b"no-such-account!", PBKDF2_ITERATIONS, 32);
}

/// Constant-time compare a candidate password against a stored hash. Returns
/// `false` for any malformed stored value (defence against schema drift).
#[cfg(feature = "server")]
pub fn verify_password(stored: &str, password: &str) -> bool {
    // Split: prefix$iterations$salt$dk
    let parts: Vec<&str> = stored.split('$').collect();
    let [prefix, iterations, salt_b64, dk_b64] = parts.as_slice() else {
        return false;
    };
    if *prefix != "pbkdf2-sha256" {
        return false;
    }
    let iterations: u32 = match iterations.parse() {
        Ok(i) => i,
        Err(_) => return false,
    };
    let salt = match b64decode(salt_b64) {
        Some(s) => s,
        None => return false,
    };
    let expected = match b64decode(dk_b64) {
        Some(d) => d,
        None => return false,
    };
    let actual = pbkdf2(password.as_bytes(), &salt, iterations, expected.len());
    constant_time_eq(&actual, &expected)
}

/// Constant-time byte comparison (avoids a length/timing oracle on the hash).
#[cfg(feature = "server")]
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(all(test, feature = "server"))]
mod pbkdf2_vectors {
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn matches_published_pbkdf2_sha256_vectors() {
        let cases: [(&[u8], &[u8], u32, usize, &str); 4] = [
            (b"password", b"salt", 1, 32, "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"),
            (b"password", b"salt", 2, 32, "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43"),
            (b"password", b"salt", 4096, 32, "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"),
            (
                b"passwordPASSWORDpassword",
                b"saltSALTsaltSALTsaltSALTsaltSALTsalt",
                4096,
                40,
                "348c89dbcbd32b2f32d814b8116e84cf2b17347ebc1800181c4e2a1fb8dd53e1c635518c7dac47e9",
            ),
        ];
        for (pw, salt, iters, len, want) in cases {
            assert_eq!(hex(&super::pbkdf2(pw, salt, iters, len)), want, "{iters} iterations");
        }
        // A key longer than the 64-byte block (HMAC hashes it first) agrees
        // with plain HMAC rounds.
        use hmac::Mac;
        let long = [7u8; 100];
        let prf = |m: &[u8]| -> [u8; 32] {
            let mut mac = <super::HmacSha256 as Mac>::new_from_slice(&long).unwrap();
            mac.update(m);
            mac.finalize().into_bytes().into()
        };
        let mut u = prf(&[b"s".as_slice(), &1u32.to_be_bytes()].concat());
        let mut want = u;
        for _ in 1..3 {
            u = prf(&u);
            for (w, x) in want.iter_mut().zip(u.iter()) {
                *w ^= *x;
            }
        }
        assert_eq!(super::pbkdf2(&long, b"s", 3, 32), want.to_vec());
    }
}
