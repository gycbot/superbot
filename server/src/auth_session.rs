//! Persistent signed login cookie for the web UI.
//!
//! After a successful HTTP Basic login, botd issues `sb_session` (Max-Age from
//! AUTH_SESSION_TTL_SECS, default 30 days; HttpOnly, Secure, SameSite=Lax).
//! The HMAC key lives in AUTH_SESSION_KEY_FILE (default $DATA_DIR/session.key,
//! mode 0600) so restarts do not invalidate sessions. The MAC is bound to the
//! current AUTH_PASSWORD, so changing the password invalidates existing cookies.

use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static KEY: OnceLock<Vec<u8>> = OnceLock::new();
static TTL: OnceLock<u64> = OnceLock::new();

const COOKIE_NAME: &str = "sb_session";
const DEFAULT_TTL: u64 = 30 * 24 * 60 * 60;

pub fn init(data_dir: &Path) {
    let ttl = std::env::var("AUTH_SESSION_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|n: &u64| *n > 0)
        .unwrap_or(DEFAULT_TTL);
    let _ = TTL.set(ttl);

    let path = std::env::var("AUTH_SESSION_KEY_FILE")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("session.key"));
    let _ = KEY.set(load_or_create_key(&path));
}

fn load_or_create_key(path: &Path) -> Vec<u8> {
    if path.exists() {
        let mut raw = Vec::new();
        std::fs::File::open(path)
            .and_then(|mut f| f.read_to_end(&mut raw))
            .unwrap_or_else(|e| panic!("read session key {}: {e}", path.display()));
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o077 != 0 {
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(path, perms);
            }
        }
        let trimmed: Vec<u8> = raw.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
        if trimmed.len() == 64 && trimmed.iter().all(|b| b.is_ascii_hexdigit()) {
            return hex_decode(&trimmed);
        }
        if !trimmed.is_empty() {
            return trimmed;
        }
        panic!("session key file {} is empty", path.display());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("create session key dir: {e}"));
    }
    let key = crate::random_bytes(32);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap_or_else(|e| panic!("write session key {}: {e}", path.display()));
    let hex = hex_encode(&key);
    f.write_all(hex.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .and_then(|_| f.sync_all())
        .expect("write session key");
    key
}

pub fn ttl() -> u64 {
    *TTL.get().unwrap_or(&DEFAULT_TTL)
}

pub fn cookie_ok(cookie_header: &str, password: &str) -> bool {
    let Some(value) = cookie_value(cookie_header) else {
        return false;
    };
    let Some((exp, mac)) = parse_value(&value) else {
        return false;
    };
    if exp <= now_ts() {
        return false;
    }
    let expected = mac_for(password, exp);
    crate::same(&mac, &expected)
}

pub fn set_cookie_header(password: &str) -> String {
    let exp = now_ts().saturating_add(ttl());
    let mac = mac_for(password, exp);
    let value = format!("v1.{exp}.{}", hex_encode(&mac));
    format!(
        "{COOKIE_NAME}={value}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
        ttl()
    )
}

fn mac_for(password: &str, exp: u64) -> [u8; 32] {
    hmac_sha256(&signing_key(password), format!("v1.{exp}").as_bytes())
}

fn signing_key(password: &str) -> [u8; 32] {
    let session_key = KEY.get().expect("session key not initialized");
    let mut buf = Vec::with_capacity(session_key.len() + 1 + password.len());
    buf.extend_from_slice(session_key);
    buf.push(0);
    buf.extend_from_slice(password.as_bytes());
    sha256(&buf)
}

fn cookie_value(header: &str) -> Option<String> {
    for part in header.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("sb_session=") {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn parse_value(v: &str) -> Option<(u64, [u8; 32])> {
    let rest = v.strip_prefix("v1.")?;
    let (exp_s, mac_s) = rest.split_once('.')?;
    let exp: u64 = exp_s.parse().ok()?;
    if mac_s.len() != 64 || !mac_s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = hex_decode(mac_s.as_bytes());
    if bytes.len() != 32 {
        return None;
    }
    let mut mac = [0u8; 32];
    mac.copy_from_slice(&bytes);
    Some((exp, mac))
}

fn now_ts() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn hex_encode(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 0xf) as usize] as char);
    }
    s
}

fn hex_decode(s: &[u8]) -> Vec<u8> {
    fn val(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
    if s.len() % 2 != 0 {
        return Vec::new();
    }
    s.chunks_exact(2).map(|c| (val(c[0]) << 4) | val(c[1])).collect()
}

fn sha256(msg: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let bit_len = (msg.len() as u64).saturating_mul(8);
    let mut data = Vec::with_capacity(msg.len() + 72);
    data.extend_from_slice(msg);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ ((!a[4]) & a[6]);
            let t1 = a[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a[7] = a[6];
            a[6] = a[5];
            a[5] = a[4];
            a[4] = a[3].wrapping_add(t1);
            a[3] = a[2];
            a[2] = a[1];
            a[1] = a[0];
            a[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(a[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Vec::with_capacity(64 + data.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(data);
    let ih = sha256(&inner);
    let mut outer = [0u8; 96];
    outer[..64].copy_from_slice(&opad);
    outer[64..].copy_from_slice(&ih);
    sha256(&outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty() {
        assert_eq!(
            hex_encode(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_abc() {
        assert_eq!(
            hex_encode(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hmac_rfc4231_case1() {
        let key = [0x0bu8; 20];
        let mac = hmac_sha256(&key, b"Hi There");
        assert_eq!(
            hex_encode(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn cookie_roundtrip_and_rejects_wrong_password() {
        let _ = KEY.set(vec![7u8; 32]);
        let _ = TTL.set(3600);
        let header = set_cookie_header("secret");
        assert!(header.contains("Max-Age=3600"));
        assert!(header.contains("HttpOnly"));
        assert!(header.contains("Secure"));
        assert!(header.contains("SameSite=Lax"));
        let pair = header.split(';').next().unwrap();
        assert!(cookie_ok(pair, "secret"));
        assert!(!cookie_ok(pair, "wrong"));
        assert!(!cookie_ok("sb_session=v1.9999999999.0000000000000000000000000000000000000000000000000000000000000000", "secret"));
        assert!(!cookie_ok("sb_session=v1.1.00", "secret"));
        assert!(!cookie_ok("", "secret"));
    }
}
